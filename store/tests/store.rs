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
        avg_ms: Some(12.0),
        min_ms: Some(10.0),
        max_ms: Some(15.0),
        jitter_ms: Some(1.5),
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
    let r = RoundResult {
        started_at: SystemTime::now(),
        down: Some(some_down()),
        up: None,
        ping_idle: Some(some_idle_ping()),
        ping_down: Some(PingStats {
            avg_ms: Some(40.0),
            min_ms: Some(20.0),
            max_ms: Some(80.0),
            jitter_ms: Some(12.0),
            loss_pct: 2.5,
            sent: 8,
        }),
        ping_up: Some(PingStats {
            avg_ms: Some(30.0),
            min_ms: Some(15.0),
            max_ms: Some(60.0),
            jitter_ms: Some(8.0),
            loss_pct: 5.0,
            sent: 8,
        }),
        skipped_reason: None,
    };
    let id = s.insert_round(&r).unwrap();
    assert!(id > 0);
    let rows = s
        .rounds_between(SystemTime::UNIX_EPOCH, SystemTime::now())
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].down_bps.is_some() && rows[0].up_bps.is_none());
    // Per-phase jitter/loss is the bufferbloat signal: idle vs under-load
    // must survive storage as six distinct values, not collapse to idle.
    assert_eq!(rows[0].jitter_idle_ms, Some(1.5));
    assert_eq!(rows[0].jitter_down_ms, Some(12.0));
    assert_eq!(rows[0].jitter_up_ms, Some(8.0));
    assert_eq!(rows[0].loss_idle_pct, Some(0.0));
    assert_eq!(rows[0].loss_down_pct, Some(2.5));
    assert_eq!(rows[0].loss_up_pct, Some(5.0));
    assert_ne!(rows[0].jitter_idle_ms, rows[0].jitter_down_ms);
    assert_ne!(rows[0].jitter_idle_ms, rows[0].jitter_up_ms);
    assert_ne!(rows[0].jitter_down_ms, rows[0].jitter_up_ms);
    assert_ne!(rows[0].loss_idle_pct, rows[0].loss_down_pct);
    assert_ne!(rows[0].loss_idle_pct, rows[0].loss_up_pct);
    assert_ne!(rows[0].loss_down_pct, rows[0].loss_up_pct);
    // CSV aliases stay the idle baseline.
    assert_eq!(rows[0].jitter_ms, rows[0].jitter_idle_ms);
    assert_eq!(rows[0].loss_pct, rows[0].loss_idle_pct);
}

/// I4. The LoL EUNE preset `104.160.142.3:443` is dead — live
/// `probe-targets` shows `no` / `-` for it. Before this fix, the same target
/// in a round produced `avg_ms: 0.0`, and because the slot was
/// `Some(PingStats)` the store wrote a real `0.0` into `ping_idle_ms`. A
/// dead target then read back as the best link on record and pulled every
/// `AVG(ping_idle_ms)` towards zero.
///
/// This goes through `stats()` rather than a hand-built `PingStats` on
/// purpose: the defect was the seam between the two, so the test has to
/// cross it.
#[test]
fn a_target_that_answers_nothing_is_stored_as_loss_never_as_a_zero_ping() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("a.db")).unwrap();
    let all_lost = alidade_engine::stats(&[
        PingSample { at: SystemTime::now(), rtt: None },
        PingSample { at: SystemTime::now(), rtt: None },
        PingSample { at: SystemTime::now(), rtt: None },
    ]);

    s.insert_round(&round_with(None, None, Some(all_lost)))
        .unwrap();

    let rows = s
        .rounds_between(SystemTime::UNIX_EPOCH, SystemTime::now())
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_ne!(
        rows[0].ping_idle_ms,
        Some(0.0),
        "a dead target was stored as a perfect 0.0 ms ping"
    );
    assert_eq!(
        rows[0].ping_idle_ms, None,
        "no answer means no RTT, so the column must be NULL"
    );
    assert_eq!(
        rows[0].loss_idle_pct,
        Some(100.0),
        "the loss itself is the measurement and must survive"
    );
    assert_eq!(rows[0].jitter_idle_ms, None);

    // NULL keeps the dead target out of the averages instead of dragging
    // them down — the second half of the same defect.
    let aggregates = s
        .round_aggregates(SystemTime::UNIX_EPOCH, SystemTime::now())
        .unwrap();
    assert_eq!(aggregates.ping_idle_ms.avg, None);
    assert_eq!(aggregates.loss_pct.avg, Some(100.0));
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

/// Samples in one minute that straddle the retention cutoff. Without flooring
/// the cutoff to a minute boundary, the first run aggregates only the samples
/// before cutoff and the second run's ON CONFLICT overwrite discards them.
#[test]
fn downsample_keeps_whole_minute_across_repeated_runs() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("a.db")).unwrap();

    // Minute-aligned unix second. Five samples: three in the first half of
    // the minute (10ms, 20ms, loss) and two in the second half (100ms, 200ms).
    const T0: u64 = 1_700_000_040;
    const RETENTION_DAYS: u32 = 30;
    let minute_start = SystemTime::UNIX_EPOCH + Duration::from_secs(T0);
    let samples = vec![
        PingSample {
            at: minute_start,
            rtt: Some(Duration::from_millis(10)),
        },
        PingSample {
            at: minute_start + Duration::from_secs(10),
            rtt: Some(Duration::from_millis(20)),
        },
        PingSample {
            at: minute_start + Duration::from_secs(20),
            rtt: None,
        },
        PingSample {
            at: minute_start + Duration::from_secs(40),
            rtt: Some(Duration::from_millis(100)),
        },
        PingSample {
            at: minute_start + Duration::from_secs(50),
            rtt: Some(Duration::from_millis(200)),
        },
    ];
    s.insert_ping_samples("google", &samples).unwrap();

    // now such that raw cutoff = T0 + 30, which sits inside the minute.
    let retention = u64::from(RETENTION_DAYS) * 86_400;
    let now1 = SystemTime::UNIX_EPOCH + Duration::from_secs(T0 + 30 + retention);
    s.downsample_pings_at(now1, RETENTION_DAYS).unwrap();

    // Advance the clock so the leftover samples in this minute become
    // eligible. Without the whole-minute cutoff, this overwrites the
    // ping_minute row with only the second-half contribution.
    let now2 = now1 + Duration::from_secs(60);
    s.downsample_pings_at(now2, RETENTION_DAYS).unwrap();

    let rows = s.ping_minute_rows().unwrap();
    assert_eq!(rows.len(), 1, "one minute bucket");
    let row = &rows[0];
    assert_eq!(row.target, "google");
    assert_eq!(row.minute, (T0 / 60) as i64);
    // All five samples: avg (10+20+100+200)/4 = 82.5, min 10, max 200, loss 1/5.
    assert_eq!(row.avg_ms, 82.5);
    assert_eq!(row.min_ms, 10.0);
    assert_eq!(row.max_ms, 200.0);
    assert_eq!(row.loss_pct, 20.0);
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
