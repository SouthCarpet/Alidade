//! Application state: `App`, `Message`, `Screen`, and the async plumbing
//! behind "Run single test" (plan Task 2).
//!
//! The engine/store interface changed the day this task was written (split
//! cadence, round kinds, load-bounded ping-under-load window — see
//! `engine/src/round.rs`), so this reads that module directly rather than
//! the plan text's now-stale interface guess.

use std::path::PathBuf;
use std::time::Duration;

use alidade_engine::{run_round, CloudflareProvider, RoundConfig, RoundResult, Settings};
use iced::Task;

/// One of the app's five top-level views. The tab strip switches between
/// them; each screen's content lands in Tasks 2, 4 and 5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    Home,
    Continuous,
    PingMonitor,
    History,
    Settings,
}

impl Screen {
    /// Tab strip order.
    pub const ALL: [Screen; 5] =
        [Screen::Home, Screen::Continuous, Screen::PingMonitor, Screen::History, Screen::Settings];

    pub fn label(&self) -> &'static str {
        match self {
            Screen::Home => "Home",
            Screen::Continuous => "Continuous",
            Screen::PingMonitor => "Ping monitor",
            Screen::History => "History",
            Screen::Settings => "Settings",
        }
    }
}

/// A finished round plus whether it actually made it into the database.
/// Kept together — rather than the plan's bare `RoundResult` — because a
/// `Store::insert_round` failure is exactly the kind of thing this release
/// exists to stop hiding (brief: "a missing measurement is displayed as
/// missing, with its reason"). The measurement itself is still shown when
/// the save fails; only the save step gets its own, separate note.
#[derive(Debug, Clone)]
pub struct RoundOutcome {
    pub result: RoundResult,
    pub persist_error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Message {
    ScreenSelected(Screen),
    /// Home: start one measurement round. Ignored while one is already
    /// running (the button is also disabled then, but `update` does not
    /// trust the view alone).
    RunSingleTest,
    RoundFinished(Box<RoundOutcome>),
}

/// The application's only long-lived state (plan 052 UI architecture: "a
/// new `app` crate ... owns the only long-lived state").
pub struct App {
    pub screen: Screen,
    pub settings: Settings,
    /// Set at boot when the settings file exists but does not parse
    /// (`engine::Settings::load_or_create` never overwrites a broken hand
    /// edit, so this app must not pretend the compiled defaults it fell
    /// back to in memory are what is actually on disk).
    pub settings_error: Option<String>,
    pub round_running: bool,
    pub last_round: Option<RoundOutcome>,
}

impl App {
    pub fn boot() -> (App, Task<Message>) {
        let (settings, settings_error) = match Settings::load_or_create(&Settings::default_path()) {
            Ok((settings, _created)) => (settings, None),
            Err(err) => (Settings::default(), Some(format!("settings: {err}"))),
        };
        (
            App {
                screen: Screen::default(),
                settings,
                settings_error,
                round_running: false,
                last_round: None,
            },
            Task::none(),
        )
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ScreenSelected(screen) => {
                self.screen = screen;
                Task::none()
            }
            Message::RunSingleTest => {
                if self.round_running {
                    return Task::none();
                }
                self.round_running = true;
                let settings = self.settings.clone();
                Task::perform(run_single_test(settings), |outcome| {
                    Message::RoundFinished(Box::new(outcome))
                })
            }
            Message::RoundFinished(outcome) => {
                self.round_running = false;
                self.last_round = Some(*outcome);
                Task::none()
            }
        }
    }
}

/// Idle-ping / phase-budget / ping-interval, mirrored from
/// `cli/src/main.rs`'s `IDLE_PING`/`PHASE_BUDGET`/`PING_INTERVAL` (not
/// exported by the engine crate — the CLI does not export its own binary
/// constants either, so this app cannot reach them any other way). Same
/// shape as the CLI's `single` command.
const IDLE_PING: Duration = Duration::from_secs(3);
const PHASE_BUDGET: Duration = Duration::from_secs(10);
const PING_INTERVAL: Duration = Duration::from_secs(1);

/// Run one round against the live Cloudflare endpoints and persist it.
/// Never byte-capped (`byte_ceiling: None`): a one-shot round the user asked
/// for by hand should not be truncated by the daily data budget, matching
/// the CLI's `single` command (`cli/src/main.rs::run_and_store`).
async fn run_single_test(settings: Settings) -> RoundOutcome {
    let provider = CloudflareProvider::new(settings.endpoints.clone());
    let cfg = RoundConfig {
        metrics: settings.metrics,
        idle_ping: IDLE_PING,
        phase_budget: PHASE_BUDGET,
        byte_ceiling: None,
        ping_interval: PING_INTERVAL,
        targets: settings
            .targets
            .iter()
            .filter(|target| target.enabled)
            .map(|target| target.probe.clone())
            .collect(),
    };
    let result = run_round(&provider, &cfg).await;
    let persist_error = persist(&result).err();
    RoundOutcome { result, persist_error }
}

/// Opens its own connection rather than sharing one held on `App` (the CLI's
/// one-shot `single` command does the same — `open_store()` per call — while
/// only its long-running `continuous` loop keeps one connection open).
fn persist(result: &RoundResult) -> Result<(), String> {
    let path = default_db_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("could not create {}: {err}", parent.display()))?;
    }
    let store = alidade_store::Store::open(&path).map_err(|err| err.to_string())?;
    store.insert_round(result).map_err(|err| err.to_string())?;
    Ok(())
}

/// Same location the CLI writes to (`cli/src/main.rs::default_db_path`):
/// `%LOCALAPPDATA%\Alidade\alidade.db`, so a round the app measures shows up
/// in `alidade export` and vice versa. Not exported by `alidade-store` (the
/// CLI does not export its own copy either), so it is mirrored here rather
/// than reaching into a sibling crate's binary.
fn default_db_path() -> PathBuf {
    let base = std::env::var_os("ALIDADE_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Alidade").join("alidade.db")
}
