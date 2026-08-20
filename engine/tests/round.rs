use alidade_engine::{
    run_round, MetricSelection, MockProvider, Probe, RoundConfig, RoundKind,
    MIN_UNDER_LOAD_SAMPLES,
};
use std::time::{Duration, Instant};

/// Ping interval and phase budget of the shape tests. The provider they use
/// has a delay, so the load lasts long enough for a real under-load series.
const PING_INTERVAL: Duration = Duration::from_millis(20);
const PHASE_BUDGET: Duration = Duration::from_millis(400);
const LOAD: Duration = Duration::from_millis(300);

fn cfg(metrics: MetricSelection) -> RoundConfig {
    RoundConfig {
        metrics,
        idle_ping: Duration::from_millis(100),
        phase_budget: PHASE_BUDGET,
        byte_ceiling: None,
        ping_interval: PING_INTERVAL,
        targets: vec![Probe::TcpConnect { host: "127.0.0.1".into(), port: 1 }], // always "lost", fine for shape tests
    }
}

/// A provider whose transfer really occupies `LOAD` of wall clock, so the
/// ping-under-load window has something to be bounded by. `MockProvider::new`
/// returns instantly, which is no load at all — see the honesty test below.
fn loaded_provider() -> MockProvider {
    MockProvider::with_delay(100_000_000.0, LOAD)
}

#[tokio::test]
async fn a_full_round_fills_every_slot_and_pings_under_load() {
    let p = loaded_provider();
    let r = run_round(&p, &cfg(MetricSelection { download: true, upload: true, ping: true })).await;
    assert_eq!(r.kind, RoundKind::Full);
    assert!(r.down.is_some() && r.up.is_some());
    assert!(r.ping_idle.is_some(), "idle ping must run first");
    assert!(r.ping_down.is_some() && r.ping_up.is_some(), "ping must continue under load");
    assert!(r.skipped_reason.is_none());
    assert!(!r.capped, "no byte ceiling was set, so nothing could cap it");
}

#[tokio::test]
async fn ping_only_round_does_no_throughput() {
    let p = loaded_provider();
    let r = run_round(&p, &cfg(MetricSelection { download: false, upload: false, ping: true })).await;
    assert_eq!(r.kind, RoundKind::PingOnly, "no throughput was asked for");
    assert!(r.down.is_none() && r.up.is_none() && r.ping_down.is_none() && r.ping_up.is_none());
    assert!(r.down_load.is_none() && r.up_load.is_none(), "no phase ran, so there is no load window");
    assert!(r.ping_idle.is_some());
}

#[tokio::test]
async fn download_only_round_keeps_upload_empty() {
    let p = MockProvider::with_delay(50_000_000.0, LOAD);
    let r = run_round(&p, &cfg(MetricSelection { download: true, upload: false, ping: true })).await;
    assert_eq!(r.kind, RoundKind::Full);
    assert!(r.down.is_some() && r.up.is_none() && r.ping_down.is_some() && r.ping_up.is_none());
}

#[tokio::test]
async fn a_provider_error_does_not_kill_the_round() {
    let p = MockProvider::failing();
    let r = run_round(&p, &cfg(MetricSelection { download: true, upload: true, ping: true })).await;
    assert!(r.down.is_none() && r.up.is_none());
    assert_eq!(
        r.kind,
        RoundKind::Full,
        "throughput was attempted and failed; that is a full round with a skip reason, not a ping round"
    );
    assert!(r.ping_idle.is_some(), "ping still ran");
    assert!(r.skipped_reason.as_deref().unwrap_or("").contains("provider"));
}

/// `MockProvider::new` resolves instantly, so a `RoundRunner` regression
/// that replaced `tokio::join!(provider.download(..), run_ping_loop(..))`
/// with two sequential `.await`s would still pass every round test above —
/// the provider call costs ~0 wall-clock time either way. `with_delay`
/// gives the provider phase real duration equal to `phase_budget`, so the
/// two shapes produce measurably different totals: concurrent is bounded by
/// `idle_ping + max(phase_budget, provider_delay)`; sequential is bounded
/// below by `idle_ping + phase_budget + provider_delay` — strictly larger
/// here since the two are equal (`max` vs `sum` of equal terms differ by a
/// full `phase_budget`). Mutation-checked: replacing the `download` phase's
/// `tokio::join!` with sequential awaits makes this fail the upper-bound
/// assertion (see task-4b-report.md).
#[tokio::test]
async fn ping_stays_concurrent_with_a_slow_provider_not_serialized_after_it() {
    let idle_ping = Duration::from_millis(80);
    let phase_budget = Duration::from_millis(400);
    let provider_delay = Duration::from_millis(400);
    let round_cfg = RoundConfig {
        metrics: MetricSelection { download: true, upload: false, ping: true },
        idle_ping,
        phase_budget,
        byte_ceiling: None,
        ping_interval: Duration::from_millis(40),
        targets: vec![Probe::TcpConnect { host: "127.0.0.1".into(), port: 1 }], // always "lost", fast to probe
    };
    let p = MockProvider::with_delay(100_000_000.0, provider_delay);

    let start = Instant::now();
    let r = run_round(&p, &round_cfg).await;
    let elapsed = start.elapsed();

    assert!(r.down.is_some(), "download must still have completed");
    assert!(r.ping_idle.is_some() && r.ping_down.is_some(), "ping must still have run");

    // Sanity floor: this must not somehow finish faster than the two
    // sequential phases (idle, then download-with-ping) could possibly
    // take even in the best case.
    assert!(
        elapsed >= idle_ping + phase_budget.min(provider_delay),
        "elapsed {elapsed:?} looks too fast to have run both phases at all"
    );
    // The discriminating bound: concurrent execution should land close to
    // idle_ping + max(phase_budget, provider_delay) (~480ms here); a
    // sequential regression lands close to idle_ping + phase_budget +
    // provider_delay (~880ms). 700ms sits with generous slack between them.
    assert!(
        elapsed < Duration::from_millis(700),
        "elapsed {elapsed:?} looks sequential (idle_ping + phase_budget + provider_delay), not concurrent — was join! replaced with sequential awaits?"
    );
}

/// Zero coverage before this task on the "ping requested but nothing is
/// configured to ping" branch (`RoundConfig::targets` empty while
/// `metrics.ping` is true) — `run_round` must record the skip with its
/// exact reason string and leave every ping slot `None`, not panic on
/// `targets.first()`.
#[tokio::test]
async fn ping_requested_with_no_targets_configured_is_a_skip_not_a_panic() {
    let p = MockProvider::new(100_000_000.0);
    let round_cfg = RoundConfig {
        metrics: MetricSelection { download: false, upload: false, ping: true },
        idle_ping: Duration::from_millis(20),
        phase_budget: Duration::from_millis(20),
        byte_ceiling: None,
        ping_interval: Duration::from_millis(10),
        targets: vec![], // ping requested, but nothing configured to ping
    };
    let r = run_round(&p, &round_cfg).await;

    assert_eq!(
        r.skipped_reason.as_deref(),
        Some("ping: no targets configured")
    );
    assert!(r.ping_idle.is_none());
    assert!(r.ping_down.is_none());
    assert!(r.ping_up.is_none());
    assert!(r.down.is_none() && r.up.is_none());
}

/// **F2, mutation-checked.** The ping-under-load loop must stop when the load
/// stops, not when `phase_budget` runs out.
///
/// This is the defect that inverted the sign of a headline metric: with a
/// 100 MiB cap the download ended after ~4.3 s of a 10 s budget, the ping
/// loop kept sampling for the full 10 s, and over half the samples measured
/// an idle link while being stored as "under load". Two of three live rounds
/// then reported latency *falling* under load.
///
/// The assertions are on the mechanism, deliberately. A rate or ping-value
/// assertion cannot discriminate here — the previous round proved a
/// plausible-looking rate assertion passes under the very mutation it was
/// written to catch. So:
///
/// * the sample count must match the load, not the budget, and
/// * the phase must RETURN when the load ends; a loop running to the budget
///   holds `run_round` open for the rest of the window.
///
/// Mutation: bound the ping loop by `phase_budget` alone (drop the
/// cancellation), and both assertions fail.
#[tokio::test]
async fn the_ping_under_load_window_ends_with_the_load_not_with_the_budget() {
    let load = Duration::from_millis(200);
    let round_cfg = RoundConfig {
        metrics: MetricSelection { download: true, upload: false, ping: true },
        idle_ping: Duration::from_millis(20),
        // Ten times the load: a loop bound by the budget runs ten times as
        // long as the transfer it claims to be measuring.
        phase_budget: Duration::from_millis(2000),
        byte_ceiling: None,
        ping_interval: Duration::from_millis(20),
        targets: vec![Probe::TcpConnect { host: "127.0.0.1".into(), port: 1 }],
    };
    let p = MockProvider::with_delay(100_000_000.0, load);

    let start = Instant::now();
    let r = run_round(&p, &round_cfg).await;
    let elapsed = start.elapsed();

    let window = r.down_load.expect("a download phase ran, so it has a load window");

    // THE mechanism assertion: the samples must fit the load window they are
    // labelled with. The loop takes at most one sample per `ping_interval`
    // of load — one cycle is a probe plus that sleep, so it is always fewer —
    // and one more may be in flight when the load ends. `+ 2` allows that one
    // and a slot of scheduling jitter; anything beyond it was taken after the
    // transfer had finished, on an idle link.
    //
    // Not a ratio of `phase_budget`, and measured rather than assumed: the
    // first version of this bound (half the budget's worth of samples) let
    // the mutation through. On this machine the fix takes 4 samples in a
    // 206 ms window and the mutation takes 17 in the same window while
    // running the full budget — 4 and 17 against a bound of 12.
    let samples_the_load_can_hold =
        (window.duration.as_millis() / round_cfg.ping_interval.as_millis()) as u32 + 2;
    assert!(
        window.ping_samples <= samples_the_load_can_hold,
        "{} under-load samples, but only {samples_the_load_can_hold} fit in the {:?} the load actually lasted — the loop is bounded by phase_budget, not by the load",
        window.ping_samples,
        window.duration
    );
    assert!(
        window.ping_samples >= MIN_UNDER_LOAD_SAMPLES,
        "{} samples is below the honesty threshold; this test needs a real series to be about the window",
        window.ping_samples
    );
    // Independent tripwire on the same mechanism: a loop that outlives the
    // transfer also holds the round open for the rest of the budget.
    assert!(
        elapsed < round_cfg.idle_ping + round_cfg.phase_budget,
        "the round took {elapsed:?}, at least the whole phase budget — the ping loop outlived the transfer it was supposed to measure"
    );
    assert!(
        window.duration < round_cfg.phase_budget / 2,
        "recorded load duration {:?} is not the {load:?} the transfer actually took",
        window.duration
    );
    assert!(r.ping_down.is_some(), "a full series must still produce a figure");
}

/// **F2 honesty rule, mutation-checked.** Fewer than `MIN_UNDER_LOAD_SAMPLES`
/// samples inside the load window means there was not enough load to measure,
/// and the under-load ping is `None` — never a number derived from one or two
/// samples of a link that was already idle.
///
/// `MockProvider::new` returns instantly, which is the extreme case of the
/// live defect: a transfer that ends the moment it starts. Exactly one sample
/// falls in that window.
///
/// Mutation: report the stats regardless of the sample count, and the first
/// assertion fails.
#[tokio::test]
async fn too_few_samples_under_load_report_no_number_at_all() {
    let round_cfg = RoundConfig {
        metrics: MetricSelection { download: true, upload: false, ping: true },
        idle_ping: Duration::from_millis(20),
        phase_budget: Duration::from_millis(500),
        byte_ceiling: None,
        ping_interval: Duration::from_millis(20),
        targets: vec![Probe::TcpConnect { host: "127.0.0.1".into(), port: 1 }],
    };
    let p = MockProvider::new(100_000_000.0); // no delay: no load at all

    let r = run_round(&p, &round_cfg).await;

    let window = r.down_load.expect("the phase ran, so its window was recorded");
    assert!(
        window.ping_samples < MIN_UNDER_LOAD_SAMPLES,
        "this test needs a window under the threshold; it held {} samples",
        window.ping_samples
    );
    assert_eq!(
        r.ping_down, None,
        "{} sample(s) under load is not a measurement — it must report None, not a number",
        window.ping_samples
    );
    assert!(
        r.down.is_some(),
        "the throughput itself is still a measurement and must survive"
    );
    assert!(
        r.ping_idle.is_some(),
        "the idle phase has no such threshold; it ran for its full duration"
    );
}

/// **F3.** With no byte ceiling — the default — nothing but the clock can end
/// a phase, and the round is not capped. The mock reports the bytes a
/// 500 Mbit/s link moves in the budget; an unconditional per-phase cap would
/// truncate that.
#[tokio::test]
async fn with_no_byte_ceiling_the_clock_alone_ends_the_phase() {
    let round_cfg = RoundConfig {
        byte_ceiling: None,
        ..cfg(MetricSelection { download: true, upload: false, ping: true })
    };
    let p = MockProvider::with_delay(500_000_000.0, LOAD);

    let r = run_round(&p, &round_cfg).await;

    let down = r.down.expect("download ran");
    // What the mock synthesizes for the whole budget, computed the same way
    // it does, so this asserts "nothing truncated it" and not float luck.
    let expected = ((500_000_000.0f64 / 8.0) * round_cfg.phase_budget.as_secs_f64()).round() as u64;
    assert_eq!(
        down.bytes, expected,
        "the phase moved less than the budget allowed, so something other than the clock stopped it"
    );
    assert!(!down.capped);
    assert!(!r.capped, "an uncapped round must never be marked capped");
}

/// **F3.** A ceiling — which can only come from a nearly-spent daily data
/// budget — still bounds the round, and a round it truncates is marked
/// `capped` so the UI and the CSV can say so instead of presenting a short
/// measurement as a normal one.
#[tokio::test]
async fn a_data_budget_ceiling_caps_the_round_and_says_so() {
    let ceiling = 1_000_000;
    let round_cfg = RoundConfig {
        byte_ceiling: Some(ceiling),
        ..cfg(MetricSelection { download: true, upload: true, ping: true })
    };
    let p = MockProvider::with_delay(500_000_000.0, LOAD); // far more than the ceiling allows

    let r = run_round(&p, &round_cfg).await;

    let down = r.down.expect("download ran");
    assert_eq!(down.bytes, ceiling, "the ceiling must bound the phase");
    assert!(down.capped, "the phase that hit the ceiling must say so");
    assert!(r.capped, "and the round must carry it out to the store");
    assert_eq!(
        r.up.expect("upload ran").bytes,
        0,
        "the download spent the whole round ceiling; upload gets what is left, which is nothing"
    );
}
