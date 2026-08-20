//! `RoundRunner`: one time-aligned measurement round (spec D4).
//!
//! A round is idle ping -> download WITH ping running -> upload WITH ping
//! running, sharing one `started_at` timestamp. `MetricSelection` decides
//! which phases actually run: an unselected phase's slot in `RoundResult`
//! stays `None` — never a zero measurement. A provider `Err` is recorded in
//! `skipped_reason` and its slot stays `None` too, but it never aborts the
//! rest of the round — ping keeps running even when throughput fails.
//!
//! Only the first configured target (`RoundConfig::targets[0]`, "primary")
//! is pinged during a round; the rest of the list is data for other engine
//! consumers, not for this module.

use std::time::{Duration, Instant, SystemTime};

use crate::{probe_once, stats, EngineError, PingSample, PingStats, Probe, SpeedProvider, Throughput};

/// Which phases of a round actually run. Unselected metrics (and, later,
/// an exhausted data budget) produce skip semantics: the matching slot in
/// `RoundResult` stays `None`, it is never reported as a zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricSelection {
    pub download: bool,
    pub upload: bool,
    pub ping: bool,
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
    /// time-bounded per spec D4, never byte-bounded.
    pub phase_budget: Duration,
    /// Hard per-phase byte ceiling, independent of `phase_budget`.
    pub max_bytes_per_phase: u64,
    /// Cadence of ping samples during the idle and ping-under-load phases.
    pub ping_interval: Duration,
    /// Ping targets for the round; only the first (primary) is used.
    pub targets: Vec<Probe>,
}

/// One round's outcome. `down`/`up` are `None` when their metric was not
/// selected or the provider errored (see `skipped_reason`). `ping_idle` is
/// `None` only when ping was not selected (or had no target); `ping_down`/
/// `ping_up` are additionally `None` when their throughput phase did not
/// run.
#[derive(Debug, Clone, PartialEq)]
pub struct RoundResult {
    pub started_at: SystemTime,
    pub down: Option<Throughput>,
    pub up: Option<Throughput>,
    pub ping_idle: Option<PingStats>,
    pub ping_down: Option<PingStats>,
    pub ping_up: Option<PingStats>,
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
    let ping_idle = match primary_target {
        Some(target) => Some(run_ping_loop(target, cfg.ping_interval, cfg.idle_ping).await),
        None => None,
    };

    // Download, with ping running concurrently when both are selected —
    // `tokio::join!` keeps the two time-aligned within the same phase
    // window (spec D4).
    let (down, ping_down) = if cfg.metrics.download {
        match primary_target {
            Some(target) => {
                let (result, ping) = tokio::join!(
                    provider.download(cfg.phase_budget, cfg.max_bytes_per_phase),
                    run_ping_loop(target, cfg.ping_interval, cfg.phase_budget),
                );
                (throughput_or_skip(result, &mut skip_reasons), Some(ping))
            }
            None => {
                let result = provider.download(cfg.phase_budget, cfg.max_bytes_per_phase).await;
                (throughput_or_skip(result, &mut skip_reasons), None)
            }
        }
    } else {
        (None, None)
    };

    // Upload — same shape as download.
    let (up, ping_up) = if cfg.metrics.upload {
        match primary_target {
            Some(target) => {
                let (result, ping) = tokio::join!(
                    provider.upload(cfg.phase_budget, cfg.max_bytes_per_phase),
                    run_ping_loop(target, cfg.ping_interval, cfg.phase_budget),
                );
                (throughput_or_skip(result, &mut skip_reasons), Some(ping))
            }
            None => {
                let result = provider.upload(cfg.phase_budget, cfg.max_bytes_per_phase).await;
                (throughput_or_skip(result, &mut skip_reasons), None)
            }
        }
    } else {
        (None, None)
    };

    RoundResult {
        started_at,
        down,
        up,
        ping_idle,
        ping_down,
        ping_up,
        skipped_reason: if skip_reasons.is_empty() {
            None
        } else {
            Some(skip_reasons.join("; "))
        },
    }
}

/// `Ok` becomes the measured `Throughput`; `Err` is recorded as
/// `"provider: <error>"` and collapses to `None` — a provider failure is
/// data (a skip), not a panic and not a bogus zero reading.
fn throughput_or_skip(
    result: Result<Throughput, EngineError>,
    skip_reasons: &mut Vec<String>,
) -> Option<Throughput> {
    match result {
        Ok(t) => Some(t),
        Err(e) => {
            skip_reasons.push(format!("provider: {e}"));
            None
        }
    }
}

/// Probe `target` every `ping_interval` until `phase_duration` has
/// elapsed (at least one sample always runs, even for a near-zero
/// duration). Loss is data, not an error — see `probe_once` — so this
/// never fails; it always returns real `PingStats`.
async fn run_ping_loop(
    target: &Probe,
    ping_interval: Duration,
    phase_duration: Duration,
) -> PingStats {
    let mut samples: Vec<PingSample> = Vec::new();
    let deadline = Instant::now() + phase_duration;
    loop {
        samples.push(probe_once(target, ping_interval).await);
        if Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(ping_interval).await;
    }
    stats(&samples)
}
