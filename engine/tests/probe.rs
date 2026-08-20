use alidade_engine::{stats, probe_once, PingSample, PingStats, Probe};
use std::time::{Duration, SystemTime};

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
