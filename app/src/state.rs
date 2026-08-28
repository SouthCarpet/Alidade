//! Application state: `App`, `Message`, `Screen`, and the async plumbing
//! behind single tests, continuous mode, ping monitor, and CSV export.

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use alidade_engine::{
    probe_once, run_round, stats, CloudflareProvider, PingSample, Probe, RoundConfig, RoundPlan,
    RoundResult, Scheduler, Settings, TargetSpec,
};
use alidade_store::RoundRow;
use iced::Task;

use crate::chart::TimeRange;
use crate::theme::Palette;

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
    pub const ALL: [Screen; 5] = [
        Screen::Home,
        Screen::Continuous,
        Screen::PingMonitor,
        Screen::History,
        Screen::Settings,
    ];

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

/// One enabled ping-monitor target, with the samples taken this session.
#[derive(Debug, Clone)]
pub struct TargetLive {
    pub name: String,
    pub probe: Probe,
    pub samples: Vec<PingSample>,
}

/// Gallery-style toast: success uses the success status colour, failure
/// uses danger. Never a blocking dialog.
#[derive(Debug, Clone)]
pub struct Toast {
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    ScreenSelected(Screen),
    /// Home: start one measurement round. Ignored while one is already
    /// running (the button is also disabled then, but `update` does not
    /// trust the view alone).
    RunSingleTest,
    RoundFinished(Box<RoundOutcome>),
    RangeSelected(TimeRange),
    Pause,
    Resume,
    ExportCsv,
    ExportFinished(Result<PathBuf, String>),
    TargetsRefreshed(Vec<(String, PingSample)>),
    Tick,
    StartPingMonitor,
    StopPingMonitor,
    HistoryFromChanged(String),
    HistoryToChanged(String),
    ChartLoaded(Result<Vec<RoundRow>, String>),
    HistoryLoaded(Result<Vec<RoundRow>, String>),
    DismissToast,
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
    pub scheduler: Scheduler,
    pub continuous_running: bool,
    pub continuous_paused: bool,
    pub time_range: TimeRange,
    pub chart_rows: Vec<RoundRow>,
    pub history_from: String,
    pub history_to: String,
    pub history_rows: Vec<RoundRow>,
    pub ping_running: bool,
    ping_in_flight: bool,
    clock_armed: bool,
    pub ping_targets: Vec<TargetLive>,
    pub toast: Option<Toast>,
    pub load_error: Option<String>,
}

impl App {
    pub fn boot() -> (App, Task<Message>) {
        let (settings, settings_error) = match Settings::load_or_create(&Settings::default_path()) {
            Ok((settings, _created)) => (settings, None),
            Err(err) => (Settings::default(), Some(format!("settings: {err}"))),
        };
        let scheduler = Scheduler::new(settings.clone());
        let (history_from, history_to) =
            crate::screens::history::default_history_range(SystemTime::now());
        (
            App {
                screen: Screen::default(),
                settings,
                settings_error,
                round_running: false,
                last_round: None,
                scheduler,
                continuous_running: false,
                continuous_paused: false,
                time_range: TimeRange::default(),
                chart_rows: Vec::new(),
                history_from: history_from.clone(),
                history_to: history_to.clone(),
                history_rows: Vec::new(),
                ping_running: false,
                ping_in_flight: false,
                clock_armed: false,
                ping_targets: Vec::new(),
                toast: None,
                load_error: None,
            },
            Task::batch([
                load_chart_task(),
                load_history_task(history_from, history_to),
            ]),
        )
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ScreenSelected(screen) => {
                self.screen = screen;
                Task::none()
            }
            Message::RunSingleTest => self.start_single_test(),
            Message::RoundFinished(outcome) => self.on_round_finished(*outcome),
            Message::RangeSelected(range) => {
                self.time_range = range;
                Task::none()
            }
            Message::Pause => {
                self.continuous_paused = true;
                self.continuous_running = false;
                Task::none()
            }
            Message::Resume => self.resume_continuous(),
            Message::ExportCsv => self.start_export(),
            Message::ExportFinished(result) => {
                self.toast = Some(toast_from_export(result));
                Task::none()
            }
            Message::TargetsRefreshed(samples) => self.on_targets_refreshed(samples),
            Message::Tick => self.on_tick(),
            Message::StartPingMonitor => self.start_ping_monitor(),
            Message::StopPingMonitor => {
                self.ping_running = false;
                Task::none()
            }
            Message::HistoryFromChanged(value) => {
                self.history_from = value;
                self.reload_history_if_valid()
            }
            Message::HistoryToChanged(value) => {
                self.history_to = value;
                self.reload_history_if_valid()
            }
            Message::ChartLoaded(result) => {
                match result {
                    Ok(rows) => {
                        self.chart_rows = rows;
                        self.load_error = None;
                    }
                    Err(err) => self.load_error = Some(err),
                }
                Task::none()
            }
            Message::HistoryLoaded(result) => {
                match result {
                    Ok(rows) => self.history_rows = rows,
                    Err(err) => {
                        self.toast = Some(Toast {
                            ok: false,
                            message: err,
                        });
                    }
                }
                Task::none()
            }
            Message::DismissToast => {
                self.toast = None;
                Task::none()
            }
        }
    }

    fn start_single_test(&mut self) -> Task<Message> {
        if self.round_running {
            return Task::none();
        }
        self.round_running = true;
        let settings = self.settings.clone();
        Task::perform(run_single_test(settings), |outcome| {
            Message::RoundFinished(Box::new(outcome))
        })
    }

    fn resume_continuous(&mut self) -> Task<Message> {
        if self.continuous_running && !self.continuous_paused {
            return Task::none();
        }
        self.screen = Screen::Continuous;
        self.continuous_running = true;
        self.continuous_paused = false;
        let mut tasks = vec![load_chart_task()];
        if !self.clock_armed {
            self.clock_armed = true;
            tasks.push(self.step_continuous());
        }
        Task::batch(tasks)
    }

    fn on_round_finished(&mut self, outcome: RoundOutcome) -> Task<Message> {
        self.round_running = false;
        let bytes = outcome
            .result
            .down_load
            .map_or(0, |window| window.bytes)
            .saturating_add(outcome.result.up_load.map_or(0, |window| window.bytes));
        self.scheduler.record_bytes(SystemTime::now(), bytes);
        self.last_round = Some(outcome);
        load_chart_task()
    }

    fn on_tick(&mut self) -> Task<Message> {
        self.clock_armed = false;
        if !self.continuous_running || self.continuous_paused {
            return Task::none();
        }
        self.clock_armed = true;
        self.step_continuous()
    }

    fn step_continuous(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        if !self.round_running {
            let now = SystemTime::now();
            if self.scheduler.next_due(now) <= now {
                tasks.push(self.begin_scheduled_round(now));
            }
        }
        tasks.push(sleep_then_tick());
        Task::batch(tasks)
    }

    fn begin_scheduled_round(&mut self, started: SystemTime) -> Task<Message> {
        self.round_running = true;
        let plan = self.scheduler.plan_next_round(started);
        self.scheduler.record_round_start(started, plan.kind);
        let settings = self.settings.clone();
        Task::perform(run_scheduled_round(settings, plan), |outcome| {
            Message::RoundFinished(Box::new(outcome))
        })
    }

    fn reload_history_if_valid(&self) -> Task<Message> {
        match crate::screens::history::parse_history_bounds(&self.history_from, &self.history_to) {
            Ok(_) => load_history_task(self.history_from.clone(), self.history_to.clone()),
            Err(_) => Task::none(),
        }
    }

    fn start_export(&mut self) -> Task<Message> {
        let (from, to) = match self.screen {
            Screen::History => match crate::screens::history::parse_history_bounds(
                &self.history_from,
                &self.history_to,
            ) {
                Ok(bounds) => bounds,
                Err(err) => {
                    self.toast = Some(Toast {
                        ok: false,
                        message: err,
                    });
                    return Task::none();
                }
            },
            _ => {
                let to = SystemTime::now();
                let from = to
                    .checked_sub(crate::screens::continuous::range_duration(self.time_range))
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                (from, to)
            }
        };
        let path = default_export_path();
        Task::perform(export_csv(from, to, path), Message::ExportFinished)
    }

    fn start_ping_monitor(&mut self) -> Task<Message> {
        if self.ping_running {
            return Task::none();
        }
        self.ping_running = true;
        self.ping_in_flight = true;
        self.ping_targets = enabled_targets(&self.settings)
            .into_iter()
            .map(|target| TargetLive {
                name: target.name.clone(),
                probe: target.probe.clone(),
                samples: Vec::new(),
            })
            .collect();
        if self.ping_targets.is_empty() {
            self.ping_running = false;
            self.ping_in_flight = false;
            self.toast = Some(Toast {
                ok: false,
                message: "no enabled targets to monitor".to_string(),
            });
            return Task::none();
        }
        self.screen = Screen::PingMonitor;
        start_ping_cycle(&self.ping_targets)
    }

    fn on_targets_refreshed(&mut self, samples: Vec<(String, PingSample)>) -> Task<Message> {
        self.ping_in_flight = false;
        for (name, sample) in samples {
            if let Some(target) = self
                .ping_targets
                .iter_mut()
                .find(|target| target.name == name)
            {
                target.samples.push(sample);
            }
        }
        if self.ping_running {
            self.ping_in_flight = true;
            start_ping_cycle(&self.ping_targets)
        } else {
            Task::none()
        }
    }
}

fn toast_from_export(result: Result<PathBuf, String>) -> Toast {
    match result {
        Ok(path) => Toast {
            ok: true,
            message: format!("Export complete: {}", path.display()),
        },
        Err(err) => Toast {
            ok: false,
            message: err,
        },
    }
}

fn enabled_targets(settings: &Settings) -> Vec<&TargetSpec> {
    settings
        .targets
        .iter()
        .filter(|target| target.enabled)
        .collect()
}

fn sleep_then_tick() -> Task<Message> {
    let wait = Palette::live_tick();
    Task::perform(async move { tokio::time::sleep(wait).await }, |_| {
        Message::Tick
    })
}

fn load_chart_task() -> Task<Message> {
    Task::perform(load_recent_rounds(), Message::ChartLoaded)
}

fn load_history_task(from: String, to: String) -> Task<Message> {
    Task::perform(
        async move { load_history_rounds(&from, &to) },
        Message::HistoryLoaded,
    )
}

fn start_ping_cycle(targets: &[TargetLive]) -> Task<Message> {
    let work: Vec<(String, Probe)> = targets
        .iter()
        .map(|target| (target.name.clone(), target.probe.clone()))
        .collect();
    Task::perform(ping_cycle(work), Message::TargetsRefreshed)
}

async fn ping_cycle(targets: Vec<(String, Probe)>) -> Vec<(String, PingSample)> {
    let started = Instant::now();
    let mut set = tokio::task::JoinSet::new();
    for (name, probe) in targets {
        set.spawn(async move { (name, probe_once(&probe, PING_INTERVAL).await) });
    }
    let mut samples = Vec::new();
    while let Some(joined) = set.join_next().await {
        if let Ok(sample) = joined {
            samples.push(sample);
        }
    }
    persist_ping_samples(&samples);
    let elapsed = started.elapsed();
    if elapsed < PING_INTERVAL {
        tokio::time::sleep(PING_INTERVAL - elapsed).await;
    }
    samples
}

fn persist_ping_samples(samples: &[(String, PingSample)]) {
    let Ok(store) = open_store() else {
        return;
    };
    for (name, sample) in samples {
        let _ = store.insert_ping_samples(name, std::slice::from_ref(sample));
    }
}

async fn load_recent_rounds() -> Result<Vec<RoundRow>, String> {
    let to = SystemTime::now();
    let from = to
        .checked_sub(crate::screens::continuous::range_duration(TimeRange::Week))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let store = open_store()?;
    store
        .rounds_between(from, to)
        .map_err(|err| err.to_string())
}

fn load_history_rounds(from_text: &str, to_text: &str) -> Result<Vec<RoundRow>, String> {
    let (from, to) = crate::screens::history::parse_history_bounds(from_text, to_text)?;
    let store = open_store()?;
    store
        .rounds_between(from, to)
        .map_err(|err| err.to_string())
}

async fn export_csv(from: SystemTime, to: SystemTime, path: PathBuf) -> Result<PathBuf, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("could not create {}: {err}", parent.display()))?;
    }
    let store = open_store()?;
    store
        .export_rounds_csv(from, to, &path)
        .map_err(|err| err.to_string())?;
    Ok(path)
}

fn default_export_path() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    downloads_dir().join(format!("alidade-rounds-{stamp}.csv"))
}

fn downloads_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Downloads")
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
    let cfg = round_config(&settings, settings.metrics, None);
    let result = run_round(&provider, &cfg).await;
    let persist_error = persist(&result).err();
    RoundOutcome {
        result,
        persist_error,
    }
}

async fn run_scheduled_round(settings: Settings, plan: RoundPlan) -> RoundOutcome {
    let provider = CloudflareProvider::new(settings.endpoints.clone());
    let cfg = round_config(&settings, plan.metrics, plan.byte_ceiling);
    let result = apply_plan(run_round(&provider, &cfg).await, &plan);
    let persist_error = persist(&result).err();
    RoundOutcome {
        result,
        persist_error,
    }
}

fn round_config(
    settings: &Settings,
    metrics: alidade_engine::MetricSelection,
    byte_ceiling: Option<u64>,
) -> RoundConfig {
    RoundConfig {
        metrics,
        idle_ping: IDLE_PING,
        phase_budget: PHASE_BUDGET,
        byte_ceiling,
        ping_interval: PING_INTERVAL,
        targets: settings
            .targets
            .iter()
            .filter(|target| target.enabled)
            .map(|target| target.probe.clone())
            .collect(),
    }
}

fn apply_plan(mut result: RoundResult, plan: &RoundPlan) -> RoundResult {
    result.kind = plan.kind;
    if result.skipped_reason.is_none() {
        result.skipped_reason.clone_from(&plan.skip_reason);
    }
    result
}

/// Opens its own connection rather than sharing one held on `App` (the CLI's
/// one-shot `single` command does the same — `open_store()` per call — while
/// only its long-running `continuous` loop keeps one connection open).
fn persist(result: &RoundResult) -> Result<(), String> {
    let store = open_store()?;
    store.insert_round(result).map_err(|err| err.to_string())?;
    Ok(())
}

fn open_store() -> Result<alidade_store::Store, String> {
    let path = default_db_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("could not create {}: {err}", parent.display()))?;
    }
    alidade_store::Store::open(&path).map_err(|err| err.to_string())
}

/// Same location the CLI writes to (`cli/src/main.rs::default_db_path`):
/// `%LOCALAPPDATA%\Alidade\alidade.db`, so a round the app measures shows up
/// in `alidade export` and vice versa. Not exported by `alidade-store` (the
/// CLI does not export its own copy either), so it is mirrored here rather
/// than reaching into a sibling crate's binary.
pub(crate) fn default_db_path() -> PathBuf {
    let base = std::env::var_os("ALIDADE_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Alidade").join("alidade.db")
}

/// Live stats for one ping-monitor target. A failing target reports no RTT,
/// never `0.0`.
pub fn target_stats(target: &TargetLive) -> alidade_engine::PingStats {
    stats(&target.samples)
}

pub fn current_rtt_ms(target: &TargetLive) -> Option<f64> {
    target
        .samples
        .last()
        .and_then(|sample| sample.rtt.map(|rtt| rtt.as_secs_f64() * 1000.0))
}

pub fn target_is_failing(target: &TargetLive) -> bool {
    match target.samples.last() {
        Some(sample) => sample.rtt.is_none(),
        None => false,
    }
}
