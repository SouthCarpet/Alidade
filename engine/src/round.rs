//! `RoundRunner`: one time-aligned measurement round (spec D4).
//!
//! A round is idle ping -> download WITH ping running -> upload WITH ping
//! running, sharing one `started_at` timestamp. `MetricSelection` decides
//! which phases actually run: an unselected phase's slot in `RoundResult`
//! stays `None` — never a zero measurement. A provider `Err` is recorded in
//! `skipped_reason` and its slot stays `None` too, but it never aborts the
//! rest of the round — ping keeps running even when throughput fails. So is
//! a phase that answered but left too little of its budget actually
//! measured to trust the rate — see [`MIN_TRUSTWORTHY_LOAD_FRACTION`]; its
//! bytes and duration still reach the store through `down_load`/`up_load`.
//!
//! A round carries its **kind** (D5a): `Full` when a throughput phase was
//! attempted, `PingOnly` when the round is the idle-ping phase alone. The
//! store persists it as `mode`, so a reader of one row knows whether a blank
//! download column means "not measured" or "never asked for".
//!
//! The ping-under-load loop is bounded by the **actual load**, not by
//! `phase_budget`. See `MIN_UNDER_LOAD_SAMPLES` and `LoadWindow` for why
//! that distinction is the difference between a bufferbloat measurement and
//! a number with the wrong sign.
//!
//! Only the first configured target (`RoundConfig::targets[0]`, "primary")
//! is pinged during a round; the rest of the list is data for other engine
//! consumers, not for this module.

use std::future::Future;
use std::time::{Duration, Instant, SystemTime};

use tokio_util::sync::CancellationToken;

use crate::{probe_once, stats, EngineError, PingSample, PingStats, Probe, SpeedProvider, Throughput};

/// Fewest ping samples inside a load window that can carry an under-load
/// figure. Below this the number is not a measurement: with one or two
/// samples a single scheduling hiccup, one retransmit, or one probe that
/// started before the transfer had ramped up **is** the average, with
/// nothing to show it as an outlier. Three is the smallest count where an
/// outlier can still be seen as one.
///
/// A phase under the threshold reports `None`, which reads "not enough load
/// to measure" — the sample count and the load duration are kept in
/// [`LoadWindow`] so that stays distinguishable from "the phase never ran".
/// The alternative, a confident number derived from one sample of a link
/// that was already idle, is exactly the defect this constant exists to
/// prevent: it made two of three live rounds report latency FALLING under
/// load.
pub const MIN_UNDER_LOAD_SAMPLES: u32 = 3;

/// Fraction of `phase_budget` a throughput phase's own measured duration
/// ([`crate::Throughput::duration`], not wall clock — see below) must reach
/// before its rate is trustworthy enough to be stored as a plain number.
///
/// This is [`MIN_UNDER_LOAD_SAMPLES`]'s reasoning applied to the OTHER
/// number a throughput phase reports. Soak row 13 moved one 25 MB chunk in
/// 1.207 s of a 10 s budget, `capped = false` (the byte ceiling was not the
/// reason it stopped — a later chunk's 429 was), and it was stored as a
/// plain 207.37 Mbit/s that read exactly like every full-window row around
/// it. The arithmetic was correct; the window was 12 % of what every other
/// row represents, and nothing said so. That is the same "too little
/// evidence" shape as one ping sample standing in for a series, and it gets
/// the same fix: below the threshold, the rate is `None` with a reason
/// recorded, never a value silently presented as if the whole window ran.
///
/// The daily budget's byte ceiling (D6) is exempt from this check — see
/// `throughput_or_skip`. It is a deliberate, already-signalled truncation
/// (`capped`, carried to the CSV) that spec D6 sanctions reading as a
/// number; this threshold is for a phase that stopped for a reason NOTHING
/// already flags.
///
/// The comparison reads `Throughput::duration`, the provider's own measured
/// clock — never a round-runner wall-clock window. For [`crate::MockProvider`]
/// (used by every round-runner test) that field always equals the requested
/// budget by construction (see `MockProvider::synthesize`), so a test double
/// that returns in near-zero wall-clock time is never mistaken for a
/// truncated phase; for [`crate::CloudflareProvider`] it excludes only the
/// per-chunk header round-trips documented on `Throughput::duration`, so a
/// real, complete phase dominated by TCP slow start is not penalised either.
///
/// One half, not tuned to row 13's 12 %: slow start and per-request overhead
/// are heaviest in a phase's first second or two, so by the halfway point
/// their share of what was measured is already small. This threshold only
/// has to separate "the deadline (or the ceiling) ended it" — both close to
/// 100 % — from "something else cut it short" — row 13's 12 %. It does not
/// need to be precise about where in between the line falls.
pub const MIN_TRUSTWORTHY_LOAD_FRACTION: f64 = 0.5;

/// Which phases of a round actually run. Unselected metrics (and an
/// exhausted data budget) produce skip semantics: the matching slot in
/// `RoundResult` stays `None`, it is never reported as a zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricSelection {
    pub download: bool,
    pub upload: bool,
    pub ping: bool,
}

/// What a round measured, structurally (spec §4 `rounds.mode`, decision
/// D5a). `Full` ran — or at least attempted — a throughput phase; `PingOnly`
/// is the idle-ping phase alone, the cheap cadence that runs every few
/// minutes between hourly throughput rounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundKind {
    Full,
    PingOnly,
}

impl RoundKind {
    /// Stable lowercase name: the `rounds.mode` column, the CSV cell and the
    /// CLI all use this one spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::PingOnly => "ping",
        }
    }

    /// Inverse of [`Self::as_str`]. `None` for anything else — an unknown
    /// mode in a stored row is corrupt data, not a value to guess at.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "full" => Some(Self::Full),
            "ping" => Some(Self::PingOnly),
            _ => None,
        }
    }
}

/// How much load a throughput phase actually put on the link, and how much
/// of the ping-under-load series fell inside it.
///
/// This exists because the under-load ping is only worth what its window is
/// worth. A phase that ended after 4 s of a 10 s budget leaves six seconds
/// of an idle link, and samples taken there are not "under load" however
/// they are labelled. Recording the window makes a short one visible instead
/// of letting it quietly poison the metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadWindow {
    /// Wall clock from the start of the phase to the moment the transfer
    /// finished — the real load, not the budget it was allowed.
    pub duration: Duration,
    /// Ping samples that STARTED while that load was running. A sample in
    /// flight when the load ends still counts: it was issued under load.
    pub ping_samples: u32,
    /// Bytes the phase actually moved, however the phase call came out.
    /// This is recorded **regardless of** whether the phase's rate was
    /// trustworthy enough to be stored as `down`/`up` (see
    /// [`MIN_TRUSTWORTHY_LOAD_FRACTION`]): a discarded rate is still real
    /// bytes that crossed the wire, and the store reads `bytes_down`/
    /// `bytes_up` from here rather than from the (possibly `None`)
    /// `Throughput`, so that evidence is never zeroed out along with a
    /// rate that was not trusted.
    pub bytes: u64,
}

/// Everything one round needs. `targets[0]` (if present) is the probe
/// pinged throughout the round; an empty `targets` with `metrics.ping =
/// true` means "ping was requested but nothing is configured to ping" and
/// is recorded as a skip, not a panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoundConfig {
    pub metrics: MetricSelection,
    /// How long the idle-ping phase runs before any throughput starts.
    pub idle_ping: Duration,
    /// Wall-clock budget for each throughput phase (download, upload) —
    /// time-bounded per spec D4.
    pub phase_budget: Duration,
    /// Byte ceiling for the WHOLE round (download first, upload gets what is
    /// left), or `None` for "the clock alone ends each phase".
    ///
    /// `None` is the default and the normal case. Per the updated D6 the
    /// daily data budget is the only thing that may end a throughput phase
    /// by byte count, so the only honest source for a `Some` here is what is
    /// left of that budget. An unconditional ceiling truncates the phase on
    /// any fast link and takes the ping-under-load window down with it.
    pub byte_ceiling: Option<u64>,
    /// Cadence of ping samples during the idle and ping-under-load phases.
    pub ping_interval: Duration,
    /// Ping targets for the round; only the first (primary) is used.
    pub targets: Vec<Probe>,
}

/// One round's outcome. `down`/`up` are `None` when their metric was not
/// selected, the provider errored, or the phase's own measured duration fell
/// under [`MIN_TRUSTWORTHY_LOAD_FRACTION`] of `phase_budget` (see
/// `skipped_reason` for which); the bytes that phase actually moved still
/// reach `down_load`/`up_load` regardless. `ping_idle` is `None` only when
/// ping was not selected (or had no target); `ping_down`/`ping_up` are
/// additionally `None` when their throughput phase did not run **or** when
/// its load window held fewer than [`MIN_UNDER_LOAD_SAMPLES`] samples —
/// `down_load`/`up_load` tell the two apart.
#[derive(Debug, Clone, PartialEq)]
pub struct RoundResult {
    pub started_at: SystemTime,
    pub kind: RoundKind,
    pub down: Option<Throughput>,
    pub up: Option<Throughput>,
    pub ping_idle: Option<PingStats>,
    pub ping_down: Option<PingStats>,
    pub ping_up: Option<PingStats>,
    /// The load window of the download phase; `None` if that phase did not
    /// run at all.
    pub down_load: Option<LoadWindow>,
    /// The load window of the upload phase; `None` if that phase did not run.
    pub up_load: Option<LoadWindow>,
    /// A byte ceiling — the daily data budget — ended a throughput phase.
    /// The round's speeds are then a truncated measurement and must be
    /// presented as such, never as a normal round.
    pub capped: bool,
    pub skipped_reason: Option<String>,
}

/// Run one round against `provider` per `cfg`. Never panics on a provider
/// error or a missing ping target — both are recorded in `skipped_reason`
/// and the round still returns a full `RoundResult`.
pub async fn run_round(provider: &dyn SpeedProvider, cfg: &RoundConfig) -> RoundResult {
    let started_at = SystemTime::now();
    let mut skip_reasons: Vec<String> = Vec::new();

    let primary_target = if cfg.metrics.ping {
        match cfg.targets.first() {
            Some(t) => Some(t),
            None => {
                skip_reasons.push("ping: no targets configured".to_string());
                None
            }
        }
    } else {
        None
    };

    // Idle ping: baseline latency measured before any throughput phase
    // starts, so it is never inflated by a phase competing for bandwidth.
    // Nothing can end this phase early, so it gets a token nobody cancels.
    let ping_idle = match primary_target {
        Some(target) => Some(
            run_ping_loop(
                target,
                cfg.ping_interval,
                cfg.idle_ping,
                &CancellationToken::new(),
            )
            .await,
        ),
        None => None,
    };

    // Download, with ping running concurrently and bounded by the download
    // itself — the moment the transfer ends, the ping loop ends with it.
    let (down, ping_down, down_load) = if cfg.metrics.download {
        let (result, ping, window) = run_loaded_phase(
            provider.download(cfg.phase_budget, cfg.byte_ceiling),
            primary_target,
            cfg.ping_interval,
            cfg.phase_budget,
        )
        .await;
        (
            throughput_or_skip(result, cfg.phase_budget, "download", &mut skip_reasons),
            ping,
            Some(window),
        )
    } else {
        (None, None, None)
    };

    // Whatever the download already spent comes off the round's ceiling, so
    // a round can never move more than the daily budget had left. This reads
    // `down_load` (bytes actually moved), never `down` (the possibly-`None`,
    // gated rate) — a discarded rate must not make the accounting think the
    // budget still has bytes back that already crossed the wire.
    let upload_ceiling = cfg
        .byte_ceiling
        .map(|ceiling| ceiling.saturating_sub(down_load.map_or(0, |w| w.bytes)));

    // Upload — same shape as download.
    let (up, ping_up, up_load) = if cfg.metrics.upload {
        let (result, ping, window) = run_loaded_phase(
            provider.upload(cfg.phase_budget, upload_ceiling),
            primary_target,
            cfg.ping_interval,
            cfg.phase_budget,
        )
        .await;
        (
            throughput_or_skip(result, cfg.phase_budget, "upload", &mut skip_reasons),
            ping,
            Some(window),
        )
    } else {
        (None, None, None)
    };

    RoundResult {
        started_at,
        kind: if cfg.metrics.download || cfg.metrics.upload {
            RoundKind::Full
        } else {
            RoundKind::PingOnly
        },
        down,
        up,
        ping_idle,
        ping_down,
        ping_up,
        down_load,
        up_load,
        capped: down.is_some_and(|t| t.capped) || up.is_some_and(|t| t.capped),
        skipped_reason: if skip_reasons.is_empty() {
            None
        } else {
            Some(skip_reasons.join("; "))
        },
    }
}

/// `Ok` becomes the measured `Throughput` — unless the phase ended for a
/// reason nothing already flags and left too little of `phase_budget`
/// actually measured (see [`MIN_TRUSTWORTHY_LOAD_FRACTION`]), in which case
/// it is recorded as `"<label>: partial reading discarded (...)"` and
/// collapses to `None`: not enough of the window ran to call it a normal
/// reading. A `capped` phase is exempt — the byte ceiling (D6) is a
/// deliberate, already-signalled truncation, sanctioned to read as a number.
///
/// `Err` is always recorded as `"<label>: provider: <error>"` and collapses
/// to `None` — a provider failure is data (a skip), not a panic and not a
/// bogus zero reading.
///
/// Either way the bytes and duration the phase actually produced are not
/// lost: they live in the caller's `LoadWindow` (`down_load`/`up_load`),
/// which is recorded regardless of what this function returns.
fn throughput_or_skip(
    result: Result<Throughput, EngineError>,
    phase_budget: Duration,
    label: &str,
    skip_reasons: &mut Vec<String>,
) -> Option<Throughput> {
    match result {
        Ok(t) if t.capped || is_trustworthy(t.duration, phase_budget) => Some(t),
        Ok(t) => {
            skip_reasons.push(format!(
                "{label}: partial reading discarded ({:.1}s of {:.1}s budget measured, {:.0}% \
                 — below the {:.0}% needed to trust the rate)",
                t.duration.as_secs_f64(),
                phase_budget.as_secs_f64(),
                load_fraction(t.duration, phase_budget) * 100.0,
                MIN_TRUSTWORTHY_LOAD_FRACTION * 100.0,
            ));
            None
        }
        Err(e) => {
            skip_reasons.push(format!("{label}: provider: {e}"));
            None
        }
    }
}

/// Share of `phase_budget` that `duration` covers. `phase_budget` is never
/// zero in practice (every `RoundConfig` in this crate is built with a real
/// budget); a zero budget reads as fully trustworthy rather than dividing by
/// zero, since there is then no window to fall short of.
fn load_fraction(duration: Duration, phase_budget: Duration) -> f64 {
    if phase_budget.is_zero() {
        1.0
    } else {
        duration.as_secs_f64() / phase_budget.as_secs_f64()
    }
}

fn is_trustworthy(duration: Duration, phase_budget: Duration) -> bool {
    load_fraction(duration, phase_budget) >= MIN_TRUSTWORTHY_LOAD_FRACTION
}

/// Run one throughput phase with the ping loop alongside it, and stop that
/// loop when the transfer stops.
///
/// This is F2, and it is a correctness fix rather than a tuning one. The
/// loop used to run for the whole `phase_budget` regardless of when the
/// transfer finished, so on a fast link most of its samples measured an idle
/// link and were stored as "under load". Two of three live rounds then
/// reported latency *falling* under load — physically impossible, and the
/// wrong sign on one of the app's two headline metrics.
///
/// `phase_budget` stays as the upper bound: it is what the transfer itself
/// is allowed, so a ping loop that outlived it would be sampling nothing
/// either way.
///
/// Returns the phase result, the under-load stats (already subject to
/// [`MIN_UNDER_LOAD_SAMPLES`]) and the [`LoadWindow`] that produced them.
async fn run_loaded_phase<F>(
    phase: F,
    target: Option<&Probe>,
    ping_interval: Duration,
    phase_budget: Duration,
) -> (
    Result<Throughput, EngineError>,
    Option<PingStats>,
    LoadWindow,
)
where
    F: Future<Output = Result<Throughput, EngineError>>,
{
    let load_ended = CancellationToken::new();
    let started = Instant::now();

    let Some(target) = target else {
        let result = phase.await;
        let bytes = result.as_ref().map_or(0, |t| t.bytes);
        return (
            result,
            None,
            LoadWindow {
                duration: started.elapsed(),
                ping_samples: 0,
                bytes,
            },
        );
    };

    // `tokio::join!` keeps the transfer and the ping series time-aligned
    // inside the same window (spec D4); the token closes that window on the
    // transfer's terms.
    let ((result, load_duration), ping) = tokio::join!(
        async {
            let result = phase.await;
            let load_duration = started.elapsed();
            load_ended.cancel();
            (result, load_duration)
        },
        run_ping_loop(target, ping_interval, phase_budget, &load_ended),
    );

    let window = LoadWindow {
        duration: load_duration,
        ping_samples: ping.sent,
        bytes: result.as_ref().map_or(0, |t| t.bytes),
    };
    let under_load = (window.ping_samples >= MIN_UNDER_LOAD_SAMPLES).then_some(ping);
    (result, under_load, window)
}

/// Probe `target` every `ping_interval` until `max_duration` has elapsed or
/// `stop` is cancelled, whichever comes first (at least one sample always
/// runs, even for a near-zero duration). Loss is data, not an error — see
/// `probe_once` — so this never fails; it always returns real `PingStats`.
async fn run_ping_loop(
    target: &Probe,
    ping_interval: Duration,
    max_duration: Duration,
    stop: &CancellationToken,
) -> PingStats {
    let mut samples: Vec<PingSample> = Vec::new();
    let deadline = Instant::now() + max_duration;
    loop {
        samples.push(probe_once(target, ping_interval).await);
        if Instant::now() >= deadline || stop.is_cancelled() {
            break;
        }
        tokio::select! {
            _ = tokio::time::sleep(ping_interval) => {}
            _ = stop.cancelled() => break,
        }
    }
    stats(&samples)
}
