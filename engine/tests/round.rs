use alidade_engine::{run_round, MetricSelection, MockProvider, Probe, RoundConfig};
use std::time::Duration;

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
