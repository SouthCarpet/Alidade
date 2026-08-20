use alidade_engine::{stats, probe_once, PingSample, PingStats, Probe};
use std::time::{Duration, Instant, SystemTime};

fn s(ms: Option<u64>) -> PingSample {
    PingSample { at: SystemTime::now(), rtt: ms.map(Duration::from_millis) }
}

#[test]
fn stats_ignore_lost_samples_for_timing_but_count_them_for_loss() {
    let st: PingStats = stats(&[s(Some(10)), s(Some(20)), s(None), s(Some(30))]);
    assert_eq!(st.sent, 4);
    assert!((st.loss_pct - 25.0).abs() < 1e-9);
    assert!((st.avg_ms - 20.0).abs() < 1e-9);
    assert!((st.min_ms - 10.0).abs() < 1e-9);
    assert!((st.max_ms - 30.0).abs() < 1e-9);
    // jitter = mean absolute difference between consecutive successful samples: |20-10|, |30-20| -> 10
    assert!((st.jitter_ms - 10.0).abs() < 1e-9);
}

#[test]
fn all_lost_is_100_percent_loss_and_zeroed_timings() {
    let st = stats(&[s(None), s(None)]);
    assert!((st.loss_pct - 100.0).abs() < 1e-9);
    assert_eq!(st.avg_ms, 0.0);
}

#[tokio::test]
async fn tcp_connect_probe_measures_a_local_listener() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { let _ = listener.accept().await; });
    let p = Probe::TcpConnect { host: "127.0.0.1".into(), port };
    let sample = probe_once(&p, Duration::from_secs(2)).await;
    assert!(sample.rtt.is_some(), "local connect must succeed");
}

#[tokio::test]
async fn tcp_connect_to_a_closed_port_is_a_loss_not_a_panic() {
    // port 1 on loopback is not listening in any normal environment
    let p = Probe::TcpConnect { host: "127.0.0.1".into(), port: 1 };
    let sample = probe_once(&p, Duration::from_millis(300)).await;
    assert!(sample.rtt.is_none());
}

#[tokio::test]
async fn icmp_probe_to_an_unresolvable_host_is_a_bounded_loss() {
    // ".invalid" is reserved by RFC 2606 to never resolve, so this never
    // touches a live target. It proves probe_once still returns within the
    // declared timeout when DNS resolution itself is slow or hangs — the
    // ICMP call's own internal timeout does not cover the resolve step, so
    // without a wrapping bound this call could hang far past `timeout`.
    let p = Probe::Icmp { host: "does-not-exist.invalid".into() };
    let timeout = Duration::from_millis(300);
    let start = Instant::now();
    let sample = probe_once(&p, timeout).await;
    let elapsed = start.elapsed();
    assert!(sample.rtt.is_none(), "an unresolvable host must be a loss");
    assert!(
        elapsed < timeout + Duration::from_secs(2),
        "elapsed {:?} was not bounded by timeout {:?}",
        elapsed,
        timeout
    );
}
