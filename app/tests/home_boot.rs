use std::time::{Duration, UNIX_EPOCH};

use alidade_app::{startup::StartupFlags, state::App};
use alidade_engine::{RoundKind, RoundResult, Settings};
use alidade_store::Store;
use tempfile::TempDir;

/// The returned `TempDir` must outlive the test: dropping it deletes the folder.
fn fixture() -> (StartupFlags, Store, TempDir) {
    let folder = tempfile::tempdir().unwrap();
    let db = folder.path().join("rounds.db");
    let settings = folder.path().join("settings.toml");
    Settings::default().save(&settings).unwrap();
    let store = Store::open(&db).unwrap();
    (
        StartupFlags {
            db: Some(db),
            settings: Some(settings),
            ..StartupFlags::default()
        },
        store,
        folder,
    )
}

fn round(seconds: u64) -> RoundResult {
    RoundResult {
        started_at: UNIX_EPOCH + Duration::from_secs(seconds),
        kind: RoundKind::PingOnly,
        down: None,
        up: None,
        ping_idle: None,
        ping_down: None,
        ping_up: None,
        down_load: None,
        up_load: None,
        capped: false,
        skipped_reason: Some("ping: no targets configured".into()),
    }
}

#[test]
fn boot_with_stored_rounds_shows_the_latest_even_outside_the_chart_range() {
    let (flags, store, _folder) = fixture();
    store.insert_round(&round(2_000)).unwrap();
    store.insert_round(&round(1_000)).unwrap();
    let (app, _) = App::boot(flags);
    let latest = app
        .last_round
        .expect("Home must show stored history after boot");
    assert_eq!(latest.started_at(), UNIX_EPOCH + Duration::from_secs(2_000));
}

#[test]
fn boot_with_an_empty_store_keeps_the_home_empty_state() {
    let (flags, _store, _folder) = fixture();
    let (app, _) = App::boot(flags);
    assert!(app.last_round.is_none());
}
