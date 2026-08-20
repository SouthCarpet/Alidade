use alidade_engine::{PingSample, PingStats, RoundResult, Throughput};
use alidade_store::Store;
use std::time::{Duration, SystemTime};

fn round_with(down: Option<Throughput>, up: Option<Throughput>, ping_idle: Option<PingStats>) -> RoundResult {
    RoundResult {
        started_at: SystemTime::now(),
        down,
        up,
        ping_idle,
        ping_down: None,
        ping_up: None,
        skipped_reason: None,
    }
}

fn some_down() -> Throughput {
    Throughput {
        bits_per_sec: 100_000_000.0,
        bytes: 1_250_000,
        duration: Duration::from_secs(1),
    }
}

fn some_idle_ping() -> PingStats {
    PingStats {
        avg_ms: 12.0,
        min_ms: 10.0,
        max_ms: 15.0,
        jitter_ms: 1.5,
        loss_pct: 0.0,
        sent: 5,
    }
}

#[test]
fn open_creates_schema_and_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.db");
    {
        let _s = Store::open(&p).unwrap();
    }
    let s = Store::open(&p).unwrap(); // second open must not fail or duplicate tables
    assert_eq!(
        s.rounds_between(SystemTime::UNIX_EPOCH, SystemTime::now())
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn round_roundtrips_with_null_metrics_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("a.db")).unwrap();
    let r = round_with(Some(some_down()), None, Some(some_idle_ping()));
    let id = s.insert_round(&r).unwrap();
    assert!(id > 0);
    let rows = s
        .rounds_between(SystemTime::UNIX_EPOCH, SystemTime::now())
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].down_bps.is_some() && rows[0].up_bps.is_none());
}

#[test]
fn downsample_collapses_raw_samples_into_minutes_and_deletes_them() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("a.db")).unwrap();
    // 120 samples inside one minute, 40 days old
    let old = SystemTime::now() - Duration::from_secs(40 * 86400);
    let samples: Vec<_> = (0..120)
        .map(|i| PingSample {
            at: old + Duration::from_millis(i * 500),
            rtt: Some(Duration::from_millis(10 + (i % 5))),
        })
        .collect();
    s.insert_ping_samples("google", &samples).unwrap();
    let moved = s.downsample_pings(30).unwrap();
    assert!(moved >= 1, "at least one minute row");
    // raw rows for that period are gone
    assert_eq!(s.ping_sample_count().unwrap(), 0);
}

#[test]
fn csv_export_writes_a_header_and_one_line_per_round() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("a.db")).unwrap();
    s.insert_round(&round_with(Some(some_down()), None, Some(some_idle_ping())))
        .unwrap();
    s.insert_round(&round_with(
        Some(Throughput {
            bits_per_sec: 50_000_000.0,
            bytes: 625_000,
            duration: Duration::from_secs(1),
        }),
        Some(Throughput {
            bits_per_sec: 20_000_000.0,
            bytes: 250_000,
            duration: Duration::from_secs(1),
        }),
        None,
    ))
    .unwrap();
    let out = dir.path().join("rounds.csv");
    let n = s
        .export_rounds_csv(SystemTime::UNIX_EPOCH, SystemTime::now(), &out)
        .unwrap();
    assert_eq!(n, 2);
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.starts_with(
        "started_at,down_mbps,up_mbps,ping_idle_ms,ping_down_ms,ping_up_ms,jitter_ms,loss_pct"
    ));
    assert_eq!(text.lines().count(), 3);
}
