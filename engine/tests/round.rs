use alidade_engine::{run_round, MetricSelection, MockProvider, Probe, RoundConfig};
use std::time::{Duration, Instant};

fn cfg(metrics: MetricSelection) -> RoundConfig {
    RoundConfig {
        metrics,
        idle_ping: Duration::from_millis(300),
        phase_budget: Duration::from_millis(300),
        max_bytes_per_phase: 1_000_000,
        ping_interval: Duration::from_millis(50),
        targets: vec![Probe::TcpConnect { host: "127.0.0.1".into(), port: 1 }], // always "lost", fine for shape tests
    }
}

#[tokio::test]
async fn a_full_round_fills_every_slot_and_pings_under_load() {
    let p = MockProvider::new(100_000_000.0);  // 100 Mbit/s
    let r = run_round(&p, &cfg(MetricSelection { download: true, upload: true, ping: true })).await;
    assert!(r.down.is_some() && r.up.is_some());
    assert!(r.ping_idle.is_some(), "idle ping must run first");
    assert!(r.ping_down.is_some() && r.ping_up.is_some(), "ping must continue under load");
    assert!(r.skipped_reason.is_none());
}

#[tokio::test]
async fn ping_only_round_does_no_throughput() {
    let p = MockProvider::new(100_000_000.0);
    let r = run_round(&p, &cfg(MetricSelection { download: false, upload: false, ping: true })).await;
    assert!(r.down.is_none() && r.up.is_none() && r.ping_down.is_none() && r.ping_up.is_none());
    assert!(r.ping_idle.is_some());
}

#[tokio::test]
async fn download_only_round_keeps_upload_empty() {
    let p = MockProvider::new(50_000_000.0);
    let r = run_round(&p, &cfg(MetricSelection { download: true, upload: false, ping: true })).await;
    assert!(r.down.is_some() && r.up.is_none() && r.ping_down.is_some() && r.ping_up.is_none());
}

#[tokio::test]
async fn a_provider_error_does_not_kill_the_round() {
    let p = MockProvider::failing();
    let r = run_round(&p, &cfg(MetricSelection { download: true, upload: true, ping: true })).await;
    assert!(r.down.is_none() && r.up.is_none());
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
        max_bytes_per_phase: 1_000_000,
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
        max_bytes_per_phase: 1_000_000,
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
