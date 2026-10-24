use std::time::{Duration, Instant, UNIX_EPOCH};

use alidade_app::screens::home::summary_facts;
use alidade_app::state::{ActivityLine, LatestRound, RoundOutcome};
use alidade_engine::{PingStats, RoundKind, RoundResult, Throughput};

fn ping(avg_ms: Option<f64>, loss_pct: f64) -> PingStats {
    PingStats {
        avg_ms,
        min_ms: avg_ms,
        max_ms: avg_ms,
        jitter_ms: None,
        loss_pct,
        sent: 5,
    }
}

fn measured() -> RoundResult {
    RoundResult {
        started_at: UNIX_EPOCH + Duration::from_secs(2_000),
        kind: RoundKind::Full,
        down: Some(Throughput {
            bits_per_sec: 123_450_000.0,
            bytes: 10,
            duration: Duration::from_secs(1),
            capped: false,
        }),
        up: Some(Throughput {
            bits_per_sec: 21_500_000.0,
            bytes: 10,
            duration: Duration::from_secs(1),
            capped: false,
        }),
        ping_idle: Some(ping(Some(12.3), 0.0)),
        ping_down: Some(ping(Some(20.0), 0.0)),
        ping_up: Some(ping(Some(40.0), 0.0)),
        down_load: None,
        up_load: None,
        capped: false,
        skipped_reason: None,
    }
}

fn live(result: RoundResult) -> LatestRound {
    LatestRound::Live(Box::new(RoundOutcome {
        result,
        persist_error: None,
    }))
}

#[test]
fn a_full_round_has_four_summary_facts_with_real_values_and_units() {
    let facts = summary_facts(&live(measured()));
    assert_eq!(
        facts.map(|fact| (fact.label, fact.text)),
        [
            ("Download", "123.45 Mbit/s".into()),
            ("Upload", "21.50 Mbit/s".into()),
            ("Idle ping", "12.3 ms".into()),
            ("Ping under load", "30.0 ms".into()),
        ]
    );
}

#[test]
fn missing_speeds_show_their_own_skip_reason_in_the_summary() {
    let mut result = measured();
    result.down = None;
    result.up = None;
    result.skipped_reason =
        Some("download: provider refused the request; upload: data budget exhausted".into());
    let facts = summary_facts(&live(result));
    assert_eq!(facts[0].text, "provider refused the request");
    assert_eq!(facts[1].text, "data budget exhausted");
}

#[test]
fn missing_idle_ping_shows_the_actual_total_loss_reason() {
    let mut result = measured();
    result.ping_idle = Some(ping(None, 100.0));
    assert_eq!(
        summary_facts(&live(result))[2].text,
        "no answer (100% loss)"
    );
}

#[test]
fn missing_loaded_ping_explains_that_no_throughput_phase_ran() {
    let mut result = measured();
    result.down = None;
    result.up = None;
    result.ping_down = None;
    result.ping_up = None;
    assert_eq!(
        summary_facts(&live(result))[3].text,
        "no throughput phase ran this round"
    );
}

#[test]
fn legacy_stored_speed_without_load_details_explains_the_missing_ping() {
    let folder = std::path::Path::new("A:/tmp/120-p3-home/tests/legacy");
    std::fs::create_dir_all(folder).unwrap();
    let db = folder.join("legacy.db");
    if db.exists() {
        std::fs::remove_file(&db).unwrap();
    }
    let store = alidade_store::Store::open(&db).unwrap();
    let mut result = measured();
    result.ping_down = None;
    result.ping_up = None;
    store.insert_round(&result).unwrap();
    let row = store
        .rounds_between(UNIX_EPOCH, result.started_at)
        .unwrap()
        .pop()
        .unwrap();
    let facts = summary_facts(&LatestRound::Stored(Box::new(row)));
    assert_eq!(facts[0].text, "123.45 Mbit/s");
    assert_eq!(facts[3].text, "load details not stored for this round");
}

#[test]
fn an_unreadable_store_reports_a_home_error_instead_of_claiming_no_rounds() {
    let folder = std::path::PathBuf::from("A:/tmp/120-p3-home/tests/unreadable");
    std::fs::create_dir_all(&folder).unwrap();
    let settings = folder.join("settings.toml");
    alidade_engine::Settings::default().save(&settings).unwrap();
    let flags = alidade_app::startup::StartupFlags {
        db: Some(folder),
        settings: Some(settings),
        ..alidade_app::startup::StartupFlags::default()
    };
    let (app, _) = alidade_app::state::App::boot(flags);
    assert!(
        app.last_round_error.is_some(),
        "a directory is not a readable round database"
    );
    assert!(app.last_round.is_none());
}

#[test]
fn idle_and_finished_activity_have_no_line() {
    let at = Instant::now();
    let mut activity = ActivityLine::new(false);
    assert!(activity.frame(at).is_none());
    activity.start(at);
    activity.stop();
    assert!(activity.frame(at).is_none());
}

#[test]
fn normal_activity_reaches_its_final_position_and_alpha_within_320_ms() {
    let at = Instant::now();
    let mut activity = ActivityLine::new(false);
    activity.start(at);
    let first = activity.frame(at).unwrap();
    assert!(first.offset < 0.0);
    assert!(first.alpha.abs() < f32::EPSILON);
    let middle = activity.frame(at + Duration::from_millis(60)).unwrap();
    assert!(middle.offset >= first.offset && middle.offset <= 0.0);
    assert!(middle.alpha > 0.0 && middle.alpha <= 1.0);
    let final_frame = activity.frame(at + Duration::from_millis(320)).unwrap();
    assert!(final_frame.offset.abs() < f32::EPSILON);
    assert!((final_frame.alpha - 1.0).abs() < f32::EPSILON);
    assert!(!activity.is_animating(at + Duration::from_millis(320)));
}

#[test]
fn reduced_motion_shows_the_final_activity_line_on_the_first_frame() {
    let at = Instant::now();
    let mut activity = ActivityLine::new(true);
    activity.start(at);
    let frame = activity.frame(at).unwrap();
    assert!(frame.offset.abs() < f32::EPSILON);
    assert!((frame.alpha - 1.0).abs() < f32::EPSILON);
    assert!(!activity.is_animating(at));
}
