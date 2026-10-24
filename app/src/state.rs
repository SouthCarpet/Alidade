//! Application state: `App`, `Message`, `Screen`, and the async plumbing
//! behind single tests, continuous mode, ping monitor, and CSV export.

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use alidade_engine::{
    probe_once, run_round, stats, CloudflareProvider, PingSample, Probe, RoundConfig, RoundPlan,
    RoundResult, Scheduler, Settings, TargetSpec,
};
use alidade_store::RoundRow;
use iced::futures::channel::mpsc;
use iced::futures::StreamExt;
use iced::{clipboard, window, Subscription, Task};

use crate::chart::TimeRange;
use crate::notify::{self, NotifyState};
use crate::screens::settings::{
    self, port_text_for, validate_new_target, NewTargetForm, SettingField, TargetField, ThemeMode,
    UiSettings,
};
use crate::startup::{ScrollTarget, StartupFlags};
use crate::theme::Palette;
use crate::tray::{self, TrayHandle, TrayMessage};
use crate::update;

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

/// Stored rows stay distinct from live results because the store does not
/// retain every sample count and duration from a live round.
#[derive(Debug, Clone)]
pub enum LatestRound {
    Live(Box<RoundOutcome>),
    Stored(Box<RoundRow>),
}

impl LatestRound {
    pub fn started_at(&self) -> SystemTime {
        match self {
            Self::Live(outcome) => outcome.result.started_at,
            Self::Stored(row) => row.started_at,
        }
    }

    pub(crate) fn capped(&self) -> bool {
        match self {
            Self::Live(outcome) => outcome.result.capped,
            Self::Stored(row) => row.capped,
        }
    }

    pub(crate) fn persist_error(&self) -> Option<&str> {
        match self {
            Self::Live(outcome) => outcome.persist_error.as_deref(),
            Self::Stored(_) => None,
        }
    }
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

/// A reversible removal keeps the original row and unparsed port input together.
#[derive(Debug, Clone)]
pub struct RemovedTarget {
    pub index: usize,
    pub target: TargetSpec,
    pub port_text: String,
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
    CopyDiagnostics,
    /// `usize` is the row's index into `App::ping_targets`, not its name —
    /// two targets sharing a name must not collapse into one row (carry-over
    /// C4 from the Task 4 verification).
    TargetsRefreshed(Vec<(usize, String, PingSample)>),
    /// Stored ping samples for the enabled targets, loaded once at boot so
    /// the Ping monitor can show real history before Start is ever pressed
    /// (F1, plan 052 Task 7 fix wave). Keyed by target name, matched against
    /// `App::ping_targets` by name rather than index — this only ever fills
    /// in a target's *starting* samples, before any live index is meaningful.
    PingHistoryLoaded(Vec<(String, Vec<PingSample>)>),
    Tick,
    StartPingMonitor,
    StopPingMonitor,
    HistoryFromChanged(String),
    HistoryToChanged(String),
    ChartLoaded(Result<Vec<RoundRow>, String>),
    HistoryLoaded(Result<Vec<RoundRow>, String>),
    DismissToast,
    /// Settings screen (plan Task 5). One editable field, rather than one
    /// message per field.
    SettingChanged(SettingField),
    TargetAdded,
    TargetRemoved(usize),
    UndoTargetRemoval,
    EscapePressed,
    UndoShortcutRequested,
    UndoShortcutChecked(bool),
    UndoRemovalExpired(u64),
    TargetToggled(usize),
    /// One field of an existing target row, committed straight to
    /// `Settings::targets[index]`.
    TargetFieldChanged(usize, TargetField),
    /// One field of the pending "add a target" form; not persisted until
    /// `TargetAdded`.
    NewTargetFieldChanged(TargetField),
    SettingsSaved(Result<(), String>),
    /// Tray icon click or menu selection (plan Task 6).
    Tray(TrayMessage),
    /// The main window's id, fetched once at boot so tray/close handling
    /// has something to hide or restore.
    WindowIdReady(Option<window::Id>),
    /// The OS asked to close the window. `App::update` decides whether that
    /// means hide-to-tray or a real exit — see `tray::should_hide_on_close`.
    WindowCloseRequested(window::Id),
    /// The OS theme's current mode, or a later change to it (Task 7
    /// pre-work A1). Only consulted when `ui_settings.theme` is
    /// `ThemeMode::System` — see [`App::palette`].
    SystemThemeChanged(iced::theme::Mode),
    /// A consented update check finished. None means no newer release.
    UpdateChecked(Option<update::Release>),
    /// Open the available release page in the browser, without a download.
    OpenReleasePage,
    AboutToggled,
    CheckUpdatesNow,
    UpdateCheckFailed(String),
    QuickThemeSwitch,
    HelpRequested,
    HelpOpened(Option<usize>),
    HelpClosed,
    HelpSearchChanged(String),
    MoveFocus(bool),
    OpenSettings,
}

/// The application's only long-lived state (plan 052 UI architecture: "a
/// new `app` crate ... owns the only long-lived state").
pub struct App {
    pub screen: Screen,
    pub settings: Settings,
    /// Where `settings.toml` was loaded from and is saved back to: the
    /// engine's own default path, or `--settings <path>` (Task 7 pre-work
    /// B1 sibling — a screenshot run with a demo DB must never write into
    /// the real user's settings file just because a setting happened to
    /// change).
    settings_path: PathBuf,
    /// Set at boot when the settings file exists but does not parse
    /// (`engine::Settings::load_or_create` never overwrites a broken hand
    /// edit, so this app must not pretend the compiled defaults it fell
    /// back to in memory are what is actually on disk).
    pub settings_error: Option<String>,
    pub round_running: bool,
    pub last_round: Option<LatestRound>,
    pub last_round_error: Option<String>,
    pub scheduler: Scheduler,
    pub continuous_running: bool,
    pub continuous_paused: bool,
    pub time_range: TimeRange,
    pub chart_rows: Vec<RoundRow>,
    pub history_from: String,
    pub history_to: String,
    pub history_rows: Vec<RoundRow>,
    pub ping_running: bool,
    clock_armed: bool,
    pub ping_targets: Vec<TargetLive>,
    pub toast: Option<Toast>,
    pub removed_target: Option<RemovedTarget>,
    removal_generation: u64,
    pub load_error: Option<String>,
    /// UI-only preferences (theme, autostart, notification thresholds) —
    /// not part of `engine::Settings`, see `screens::settings`'s module doc.
    pub ui_settings: UiSettings,
    /// Raw text buffers for the numeric fields on the Settings screen. Kept
    /// separate from the parsed value so the user can type a transient
    /// invalid value (an empty field, a stray letter) without it being
    /// discarded or committed early — the same shape History's `from`/`to`
    /// buffers already use.
    pub interval_text: String,
    pub budget_gb_text: String,
    pub notify_ping_ms_text: String,
    pub notify_download_mbit_text: String,
    pub notify_loss_pct_text: String,
    /// Port text buffers for existing target rows, aligned by index with
    /// `settings.targets` (name/host/kind are free-form text with no
    /// invalid state, so only the numeric port needs a buffer).
    pub target_port_text: Vec<String>,
    pub new_target: NewTargetForm,
    /// Kept alive for the tray icon's lifetime (plan Task 6); `None` when
    /// the tray failed to build (non-Windows, or the OS refused it) — see
    /// `tray::should_hide_on_close`, which turns close-to-tray off then.
    tray: Option<TrayHandle>,
    /// The main window's id, learned once at boot. `None` only in the brief
    /// window before that lookup resolves.
    window_id: Option<window::Id>,
    /// `--minimized` (Task 6 Step 2): applied once, the first time
    /// `window_id` becomes known.
    start_minimized: bool,
    /// Threshold-notification cooldown state (Task 6 Step 3).
    notify_state: NotifyState,
    /// The OS theme mode, as last reported by `iced::system::theme_changes`
    /// (Task 7 pre-work A1). `Mode::None` until the boot-time query answers,
    /// which `App::palette` treats the same as `Mode::Light`.
    system_theme: iced::theme::Mode,
    /// A newer GitHub release, set once `Message::UpdateChecked` arrives
    /// (plan 064 Task 2). `None` until then, and stays `None` when the
    /// check is off, finds nothing newer, or fails.
    pub available_update: Option<update::Release>,
    pub help_open: bool,
    pub help_query: String,
    help_opener: Option<usize>,
    pub about_open: bool,
    pub update_checking: bool,
    pub last_update_result: Option<String>,
    pub last_update_at: Option<SystemTime>,
}

impl App {
    pub fn boot(flags: StartupFlags) -> (App, Task<Message>) {
        // `--settings <path>` (F1, plan 052 Task 7 fix wave): a demo DB and
        // the real default settings file can name different targets, which
        // was the root cause of the Ping monitor showing none — a
        // screenshot run now points both at the same place.
        let settings_path = flags
            .settings
            .clone()
            .unwrap_or_else(Settings::default_path);
        let (settings, settings_error) = match Settings::load_or_create(&settings_path) {
            Ok((settings, _created)) => (settings, None),
            Err(err) => (Settings::default(), Some(format!("settings: {err}"))),
        };
        let scheduler = Scheduler::new(settings.clone());
        let (history_from, history_to) =
            crate::screens::history::default_history_range(SystemTime::now());
        let ui_settings_path = UiSettings::path_beside(&settings_path);
        let (mut ui_settings, ui_settings_error) = UiSettings::load_or_default(&ui_settings_path);
        // `--theme` (Task 7 pre-work B1) overrides the loaded choice for this
        // run only — it is never written back to `ui-settings.toml` (B1:
        // "flags only affect startup; no new persistent state").
        if let Some(theme) = flags.theme {
            ui_settings.theme = theme;
        }
        let interval_text = (settings.throughput_every.as_secs() / 60).to_string();
        let budget_gb_text = settings
            .daily_budget_bytes
            .map(settings::bytes_to_gb_text)
            .unwrap_or_else(|| "5.0".to_string());
        let notify_ping_ms_text = ui_settings.notify_ping_ms.value.to_string();
        let notify_download_mbit_text = ui_settings.notify_download_mbit.value.to_string();
        let notify_loss_pct_text = ui_settings.notify_loss_pct.value.to_string();
        let target_port_text = settings.targets.iter().map(port_text_for).collect();
        // F1 (Task 7 fix wave): the Ping monitor lists every enabled target
        // straight from settings, before Start is ever pressed — samples
        // (if any) are filled in afterwards by `load_ping_history_task`.
        let ping_targets = sync_ping_targets(&[], &settings);
        let ping_history_task = load_ping_history_task(&settings);
        let scroll_task = match flags.scroll {
            Some(ScrollTarget::SettingsNotifications) => scroll_settings_to_notifications_task(),
            None => Task::none(),
        };
        // Spawned here (App::boot runs on the winit thread before `.run()`
        // starts pumping it — see tray.rs's module doc) so the tray's
        // hidden window shares the same message queue the real window uses.
        let (tray_sender, tray_receiver) = mpsc::unbounded();
        let (tray, tray_error) = match tray::spawn(tray_sender) {
            Ok(handle) => (Some(handle), None),
            Err(err) => (None, Some(err.to_string())),
        };
        // A broken settings file is the louder problem when both happen at
        // once; the tray failing is still surfaced, just not silently.
        let toast = ui_settings_error
            .map(|message| Toast { ok: false, message })
            .or_else(|| tray_error.map(|message| Toast { ok: false, message }));
        // `--screen` (B1) picks the initial tab; unset falls back to Home.
        let screen = flags.screen.unwrap_or_default();
        let (last_round, last_round_error) =
            match load_latest_round(&flags.db.unwrap_or_else(default_db_path)) {
                Ok(round) => (round, None),
                Err(error) => (None, Some(error)),
            };
        let mut app = App {
            screen,
            settings,
            settings_path,
            settings_error,
            round_running: false,
            last_round,
            last_round_error,
            scheduler,
            continuous_running: false,
            continuous_paused: false,
            time_range: TimeRange::default(),
            chart_rows: Vec::new(),
            history_from: history_from.clone(),
            history_to: history_to.clone(),
            history_rows: Vec::new(),
            ping_running: false,
            clock_armed: false,
            ping_targets,
            toast,
            removed_target: None,
            removal_generation: 0,
            load_error: None,
            ui_settings,
            interval_text,
            budget_gb_text,
            notify_ping_ms_text,
            notify_download_mbit_text,
            notify_loss_pct_text,
            target_port_text,
            new_target: NewTargetForm::default(),
            tray,
            window_id: None,
            start_minimized: flags.minimized,
            notify_state: NotifyState::default(),
            system_theme: iced::theme::Mode::None,
            available_update: None,
            help_open: false,
            help_query: String::new(),
            help_opener: None,
            about_open: false,
            update_checking: false,
            last_update_result: None,
            last_update_at: None,
        };
        let update_task = app.request_update_check();
        (
            app,
            Task::batch([
                load_chart_task(),
                load_history_task(history_from, history_to),
                ping_history_task,
                scroll_task,
                update_task,
                Task::stream(tray_receiver.map(Message::Tray)),
                window::oldest().map(Message::WindowIdReady),
                iced::system::theme().map(Message::SystemThemeChanged),
            ]),
        )
    }

    /// The active [`Palette`] (Task 7 pre-work A1): resolved once here from
    /// `ui_settings.theme` and handed to every screen's `view` — the one
    /// place in the app that turns a theme choice into an actual palette, so
    /// no screen can hardcode `Palette::light()` and quietly disagree with
    /// the rest. `System` follows `system_theme`, the OS mode iced reports
    /// through `iced::system::theme_changes`; `Mode::None` (not yet known,
    /// or the OS did not say) reads as light, the same default the token
    /// mode enum itself uses.
    pub fn palette(&self) -> Palette {
        match self.ui_settings.theme {
            ThemeMode::Light => Palette::light(),
            ThemeMode::Dark => Palette::dark(),
            ThemeMode::System => match self.system_theme {
                iced::theme::Mode::Dark => Palette::dark(),
                iced::theme::Mode::Light | iced::theme::Mode::None => Palette::light(),
            },
        }
    }

    /// Window, OS theme, and keyboard events stay subscribed on every screen.
    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            window::close_requests().map(Message::WindowCloseRequested),
            iced::system::theme_changes().map(Message::SystemThemeChanged),
            iced::event::listen_with(|event, _, _| match event {
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key, modifiers, ..
                }) => shortcut(key, modifiers),
                _ => None,
            }),
        ])
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
            Message::CopyDiagnostics => self.copy_diagnostics(),
            Message::TargetsRefreshed(samples) => self.on_targets_refreshed(samples),
            Message::PingHistoryLoaded(history) => self.on_ping_history_loaded(history),
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
            Message::SettingChanged(field) => self.apply_setting_change(field),
            Message::TargetAdded => self.add_target(),
            Message::TargetRemoved(index) => self.remove_target(index),
            Message::UndoShortcutRequested => {
                crate::screens::help::text_input_focused().map(Message::UndoShortcutChecked)
            }
            Message::UndoShortcutChecked(true) => Task::none(),
            Message::UndoShortcutChecked(false) => self.update(Message::UndoTargetRemoval),
            Message::UndoTargetRemoval => {
                if self.help_open {
                    Task::none()
                } else {
                    self.undo_target_removal()
                }
            }
            Message::UndoRemovalExpired(generation) => {
                if generation == self.removal_generation {
                    self.removed_target = None;
                }
                Task::none()
            }
            Message::TargetToggled(index) => self.toggle_target(index),
            Message::TargetFieldChanged(index, field) => self.edit_target(index, field),
            Message::NewTargetFieldChanged(field) => {
                self.new_target.apply(field);
                Task::none()
            }
            Message::SettingsSaved(result) => {
                if let Err(err) = result {
                    self.toast = Some(Toast {
                        ok: false,
                        message: format!("settings not saved: {err}"),
                    });
                }
                Task::none()
            }
            Message::Tray(message) => self.on_tray_message(message),
            Message::WindowIdReady(id) => self.on_window_id_ready(id),
            Message::WindowCloseRequested(id) => self.on_close_requested(id),
            Message::SystemThemeChanged(mode) => {
                self.system_theme = mode;
                Task::none()
            }
            Message::UpdateChecked(release) => self.finish_update_check(Ok(release)),
            Message::UpdateCheckFailed(error) => self.finish_update_check(Err(error)),
            Message::CheckUpdatesNow => self.request_update_check(),
            Message::AboutToggled => {
                self.about_open = !self.about_open;
                if self.about_open {
                    self.request_update_check()
                } else {
                    Task::none()
                }
            }
            Message::HelpRequested => {
                if self.help_open {
                    return Task::none();
                }
                crate::screens::help::capture_focus().map(Message::HelpOpened)
            }
            Message::HelpOpened(opener) => self.open_help(opener),
            Message::HelpClosed => self.close_help(),
            Message::EscapePressed => self.escape(),
            Message::HelpSearchChanged(query) => {
                self.help_query = query;
                Task::none()
            }
            Message::MoveFocus(backwards) => self.move_focus(backwards),
            Message::OpenSettings => {
                if !self.help_open {
                    self.screen = Screen::Settings;
                }
                Task::none()
            }
            Message::QuickThemeSwitch => {
                let mode = if self.palette().dark {
                    ThemeMode::Light
                } else {
                    ThemeMode::Dark
                };
                self.apply_setting_change(SettingField::Theme(mode))
            }
            Message::OpenReleasePage => self.open_release_page(),
        }
    }

    fn open_help(&mut self, opener: Option<usize>) -> Task<Message> {
        if self.help_open {
            return Task::none();
        }
        self.help_opener = opener;
        self.help_query.clear();
        self.help_open = true;
        iced::widget::operation::focus(crate::screens::help::search_id())
    }

    fn escape(&mut self) -> Task<Message> {
        if !self.help_open {
            self.removed_target = None;
            return Task::none();
        }
        if !self.help_query.is_empty() {
            self.help_query.clear();
            return Task::none();
        }
        self.close_help()
    }

    fn close_help(&mut self) -> Task<Message> {
        if !self.help_open {
            return Task::none();
        }
        self.help_open = false;
        self.help_query.clear();
        crate::screens::help::restore_focus(self.help_opener.take())
    }

    fn move_focus(&self, backwards: bool) -> Task<Message> {
        if self.help_open {
            return iced::widget::operation::focus(crate::screens::help::search_id());
        }
        if backwards {
            iced::widget::operation::focus_previous()
        } else {
            iced::widget::operation::focus_next()
        }
    }

    // All entry points pass the same consent gate before creating network work.
    fn request_update_check(&mut self) -> Task<Message> {
        if !wants_update_check(&self.ui_settings) || self.update_checking {
            return Task::none();
        }
        self.update_checking = true;
        update_check_task()
    }

    fn finish_update_check(
        &mut self,
        result: Result<Option<update::Release>, String>,
    ) -> Task<Message> {
        self.update_checking = false;
        if !wants_update_check(&self.ui_settings) {
            return Task::none();
        }
        self.last_update_at = Some(SystemTime::now());
        self.last_update_result = Some(match result {
            Ok(release) => {
                let text = match &release {
                    Some(release) => format!("Last result: version {} is available.", release.tag),
                    None => "Last result: you have the latest version.".to_string(),
                };
                self.available_update = release;
                text
            }
            Err(error) => format!("Last result: check failed ({error})."),
        });
        Task::none()
    }

    /// Current update-check feedback, including the opt-out state.
    pub fn update_status(&self) -> &str {
        if !self.ui_settings.check_updates {
            "Update checks are off. Enable them below to check GitHub."
        } else if self.update_checking {
            "Checking GitHub..."
        } else {
            self.last_update_result
                .as_deref()
                .unwrap_or("No check in this session.")
        }
    }

    /// Task 6 Step 1/2: the tray menu reuses the same handling as the
    /// matching in-window action (Pause, Run single test) rather than
    /// duplicating it, so the two can never drift apart.
    fn on_tray_message(&mut self, message: TrayMessage) -> Task<Message> {
        match message {
            TrayMessage::ShowWindow => self.restore_window(),
            TrayMessage::RunSingleTest => self.update(Message::RunSingleTest),
            TrayMessage::PauseContinuous => self.update(Message::Pause),
            // The only real exit (Task 6 Step 2) — it does not go through
            // `should_hide_on_close` at all.
            TrayMessage::Quit => iced::exit(),
        }
    }

    fn on_window_id_ready(&mut self, id: Option<window::Id>) -> Task<Message> {
        self.window_id = id;
        match (id, self.start_minimized) {
            (Some(id), true) => {
                self.start_minimized = false;
                window::set_mode(id, window::Mode::Hidden)
            }
            _ => Task::none(),
        }
    }

    fn restore_window(&self) -> Task<Message> {
        match self.window_id {
            Some(id) => window::set_mode(id, window::Mode::Windowed),
            None => Task::none(),
        }
    }

    /// Opens the pending release's page in the browser. Never downloads or
    /// installs anything; a failure to launch a browser is not worth a
    /// toast over.
    fn open_release_page(&self) -> Task<Message> {
        if let Some(release) = &self.available_update {
            let _ = open::that(&release.url);
        }
        Task::none()
    }

    /// Task 6 Step 2: hide to the tray instead of exiting, except when
    /// there is no tray to hide to (see `tray::should_hide_on_close`).
    fn on_close_requested(&self, id: window::Id) -> Task<Message> {
        if tray::should_hide_on_close(self.tray.is_some(), self.continuous_running) {
            window::set_mode(id, window::Mode::Hidden)
        } else {
            window::close(id)
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
        self.notify_thresholds(&outcome.result);
        self.last_round = Some(LatestRound::Live(Box::new(outcome)));
        self.last_round_error = None;
        load_chart_task()
    }

    /// Task 6 Step 3: every finished round (single test or scheduled) gets a
    /// chance to breach a threshold. `evaluate` is the pure decision; `show`
    /// is the one line that actually raises a toast.
    fn notify_thresholds(&mut self, result: &RoundResult) {
        for notification in notify::evaluate(
            &self.ui_settings,
            result,
            &mut self.notify_state,
            Instant::now(),
            notify::DEFAULT_COOLDOWN,
        ) {
            notify::show(&notification);
        }
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

    /// C2 (Task 4 verification): an unparseable From/To used to be dropped
    /// silently. Now the user sees why the table did not refresh.
    fn reload_history_if_valid(&mut self) -> Task<Message> {
        match crate::screens::history::parse_history_bounds(&self.history_from, &self.history_to) {
            Ok(_) => load_history_task(self.history_from.clone(), self.history_to.clone()),
            Err(err) => {
                self.toast = Some(Toast {
                    ok: false,
                    message: err,
                });
                Task::none()
            }
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

    fn copy_diagnostics(&mut self) -> Task<Message> {
        let (from, to) = match crate::screens::history::parse_history_bounds(
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
        };
        let summary =
            crate::diagnostics::summary(&self.history_rows, (from, to), env!("CARGO_PKG_VERSION"));
        self.toast = Some(Toast {
            ok: true,
            message: "Diagnostics copied".to_string(),
        });
        clipboard::write(summary)
    }

    fn start_ping_monitor(&mut self) -> Task<Message> {
        if self.ping_running {
            return Task::none();
        }
        self.ping_running = true;
        // F1 (Task 7 fix wave): re-sync rather than rebuild from scratch, so
        // Start keeps whatever history the row already shows (from settings
        // plus the store) instead of wiping it back to empty.
        self.ping_targets = sync_ping_targets(&self.ping_targets, &self.settings);
        if self.ping_targets.is_empty() {
            self.ping_running = false;
            self.toast = Some(Toast {
                ok: false,
                message: "no enabled targets to monitor".to_string(),
            });
            return Task::none();
        }
        self.screen = Screen::PingMonitor;
        start_ping_cycle(&self.ping_targets)
    }

    /// C4 (Task 4 verification): samples used to be matched back to a target
    /// by name, so two targets sharing a name collapsed into one row. The
    /// index is stable for the lifetime of one monitor session (the vector is
    /// only rebuilt in `start_ping_monitor`), so keying by it — rather than
    /// requiring unique names in `Settings` — fixes the collapse without a
    /// new validation surface in the targets editor.
    fn on_targets_refreshed(&mut self, samples: Vec<(usize, String, PingSample)>) -> Task<Message> {
        for (index, _name, sample) in samples {
            if let Some(target) = self.ping_targets.get_mut(index) {
                target.samples.push(sample);
            }
        }
        if self.ping_running {
            start_ping_cycle(&self.ping_targets)
        } else {
            Task::none()
        }
    }

    /// F1 (Task 7 fix wave): fills in a target's starting samples from the
    /// store, matched by name. Only when the row is still empty — a live
    /// sample that already arrived (the live loop is much faster than this
    /// one-shot load) must never be overwritten by older, stored history.
    fn on_ping_history_loaded(&mut self, history: Vec<(String, Vec<PingSample>)>) -> Task<Message> {
        for (name, samples) in history {
            if samples.is_empty() {
                continue;
            }
            if let Some(target) = self
                .ping_targets
                .iter_mut()
                .find(|target| target.name == name)
            {
                if target.samples.is_empty() {
                    target.samples = samples;
                }
            }
        }
        Task::none()
    }

    // -------------------------------------------------------------
    // Settings screen (plan Task 5)
    // -------------------------------------------------------------

    /// Every branch that actually changes `self.settings` also pushes it
    /// into the running `Scheduler` (Step 4: "the scheduler picks up the new
    /// interval on the next round without restart") and then persists both
    /// settings files. A branch that only updates a text buffer without a
    /// valid parse (an interval still being typed, say) does neither yet.
    fn apply_setting_change(&mut self, field: SettingField) -> Task<Message> {
        match field {
            SettingField::IntervalMinutes(text) => {
                self.interval_text = text.clone();
                match settings::parse_interval_minutes(&text) {
                    Ok(duration) => {
                        self.settings.throughput_every = duration;
                        return self.commit_settings();
                    }
                    Err(err) => {
                        self.toast = Some(Toast {
                            ok: false,
                            message: err,
                        })
                    }
                }
            }
            SettingField::Metric(kind, checked) => {
                match settings::toggle_metric(self.settings.metrics, kind, checked) {
                    Some(metrics) => {
                        self.settings.metrics = metrics;
                        return self.commit_settings();
                    }
                    None => {
                        self.toast = Some(Toast {
                            ok: false,
                            message: "at least one metric must stay selected".to_string(),
                        });
                    }
                }
            }
            SettingField::Theme(mode) => {
                self.ui_settings.theme = mode;
                return self.persist_ui_settings();
            }
            SettingField::Autostart(value) => {
                self.ui_settings.autostart = value;
                return self.persist_ui_settings();
            }
            SettingField::CheckUpdates(value) => {
                self.ui_settings.check_updates = value;
                if !value {
                    self.available_update = None;
                    self.update_checking = false;
                }
                return self.persist_ui_settings();
            }
            SettingField::BudgetEnabled(enabled) => {
                self.settings.daily_budget_bytes = if enabled {
                    Some(settings::parse_gb_to_bytes(&self.budget_gb_text).unwrap_or(5_000_000_000))
                } else {
                    None
                };
                return self.commit_settings();
            }
            SettingField::BudgetGb(text) => {
                self.budget_gb_text = text.clone();
                match settings::parse_gb_to_bytes(&text) {
                    Ok(bytes) if self.settings.daily_budget_bytes.is_some() => {
                        self.settings.daily_budget_bytes = Some(bytes);
                        return self.commit_settings();
                    }
                    Ok(_) => {}
                    Err(err) => {
                        self.toast = Some(Toast {
                            ok: false,
                            message: err,
                        })
                    }
                }
            }
            SettingField::NotifyMaster(value) => {
                self.ui_settings.notify_master = value;
                return self.persist_ui_settings();
            }
            SettingField::NotifyPingEnabled(value) => {
                self.ui_settings.notify_ping_ms.enabled = value;
                return self.persist_ui_settings();
            }
            SettingField::NotifyPingMs(text) => {
                self.notify_ping_ms_text = text.clone();
                if let Ok(value) = text.trim().parse() {
                    self.ui_settings.notify_ping_ms.value = value;
                    return self.persist_ui_settings();
                }
            }
            SettingField::NotifyDownloadEnabled(value) => {
                self.ui_settings.notify_download_mbit.enabled = value;
                return self.persist_ui_settings();
            }
            SettingField::NotifyDownloadMbit(text) => {
                self.notify_download_mbit_text = text.clone();
                if let Ok(value) = text.trim().parse() {
                    self.ui_settings.notify_download_mbit.value = value;
                    return self.persist_ui_settings();
                }
            }
            SettingField::NotifyLossEnabled(value) => {
                self.ui_settings.notify_loss_pct.enabled = value;
                return self.persist_ui_settings();
            }
            SettingField::NotifyLossPct(text) => {
                self.notify_loss_pct_text = text.clone();
                if let Ok(value) = text.trim().parse() {
                    self.ui_settings.notify_loss_pct.value = value;
                    return self.persist_ui_settings();
                }
            }
        }
        Task::none()
    }

    fn add_target(&mut self) -> Task<Message> {
        match validate_new_target(&self.new_target) {
            Ok(target) => {
                self.target_port_text.push(port_text_for(&target));
                self.settings.targets.push(target);
                self.new_target = NewTargetForm::default();
                self.commit_settings()
            }
            Err(err) => {
                self.toast = Some(Toast {
                    ok: false,
                    message: err,
                });
                Task::none()
            }
        }
    }

    fn remove_target(&mut self, index: usize) -> Task<Message> {
        if index >= self.settings.targets.len() {
            return Task::none();
        }
        let target = self.settings.targets.remove(index);
        let port_text = if index < self.target_port_text.len() {
            self.target_port_text.remove(index)
        } else {
            port_text_for(&target)
        };
        self.removed_target = Some(RemovedTarget {
            index,
            target,
            port_text,
        });
        self.removal_generation = self.removal_generation.wrapping_add(1);
        let generation = self.removal_generation;
        let expiry = Task::perform(
            async move {
                tokio::time::sleep(Duration::from_secs(10)).await;
                generation
            },
            Message::UndoRemovalExpired,
        );
        Task::batch([self.commit_settings(), expiry])
    }

    fn undo_target_removal(&mut self) -> Task<Message> {
        let Some(removed) = self.removed_target.take() else {
            return Task::none();
        };
        let index = removed.index.min(self.settings.targets.len());
        self.settings.targets.insert(index, removed.target);
        self.target_port_text.insert(index, removed.port_text);
        self.commit_settings()
    }

    fn toggle_target(&mut self, index: usize) -> Task<Message> {
        let Some(target) = self.settings.targets.get_mut(index) else {
            return Task::none();
        };
        target.enabled = !target.enabled;
        self.commit_settings()
    }

    fn edit_target(&mut self, index: usize, field: TargetField) -> Task<Message> {
        if let TargetField::Port(text) = &field {
            if let Some(buffer) = self.target_port_text.get_mut(index) {
                *buffer = text.clone();
            }
        }
        let port_text = self
            .target_port_text
            .get(index)
            .cloned()
            .unwrap_or_default();
        let Some(target) = self.settings.targets.get_mut(index) else {
            return Task::none();
        };
        match field {
            TargetField::Name(text) => target.name = text,
            TargetField::Host(text) => settings::set_probe_host(&mut target.probe, text),
            TargetField::Kind(kind) => {
                let host = settings::probe_host(&target.probe).to_string();
                target.probe = settings::build_probe(kind, host, &port_text);
            }
            TargetField::Port(text) => match (text.trim().parse::<u16>(), &mut target.probe) {
                (Ok(port), Probe::TcpConnect { port: existing, .. }) => *existing = port,
                (Err(_), _) => {
                    self.toast = Some(Toast {
                        ok: false,
                        message: format!("port must be a number from 1 to 65535, got `{text}`"),
                    });
                    return Task::none();
                }
                _ => {}
            },
        }
        self.commit_settings()
    }

    /// Push the changed `self.settings` into the running scheduler (so the
    /// next round already sees it — Step 4), resync the Ping monitor's
    /// target list (F1, Task 7 fix wave — a target add/remove/toggle must
    /// show up there immediately, not just after Start), and persist both
    /// settings files.
    fn commit_settings(&mut self) -> Task<Message> {
        self.scheduler.update_settings(self.settings.clone());
        self.ping_targets = sync_ping_targets(&self.ping_targets, &self.settings);
        self.persist_settings()
    }

    fn persist_settings(&self) -> Task<Message> {
        let settings = self.settings.clone();
        let ui_settings = self.ui_settings.clone();
        let settings_path = self.settings_path.clone();
        Task::perform(
            async move {
                settings
                    .save(&settings_path)
                    .map_err(|err| err.to_string())?;
                ui_settings.save(&UiSettings::path_beside(&settings_path))
            },
            Message::SettingsSaved,
        )
    }

    /// Same as `persist_settings`, without re-writing the engine's own
    /// settings file when only a UI-only preference changed.
    fn persist_ui_settings(&self) -> Task<Message> {
        let ui_settings = self.ui_settings.clone();
        let settings_path = self.settings_path.clone();
        Task::perform(
            async move { ui_settings.save(&UiSettings::path_beside(&settings_path)) },
            Message::SettingsSaved,
        )
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

/// Rebuilds the Ping monitor's live target list from `settings`'s enabled
/// targets (F1, plan 052 Task 7 fix wave: the list is populated from
/// settings directly, not only once Start is pressed). Any target whose
/// name matches one already in `existing` keeps its samples — a settings
/// edit, or pressing Start, must never throw away history the row is
/// already showing.
fn sync_ping_targets(existing: &[TargetLive], settings: &Settings) -> Vec<TargetLive> {
    enabled_targets(settings)
        .into_iter()
        .map(|target| {
            let samples = existing
                .iter()
                .find(|live| live.name == target.name)
                .map(|live| live.samples.clone())
                .unwrap_or_default();
            TargetLive {
                name: target.name.clone(),
                probe: target.probe.clone(),
                samples,
            }
        })
        .collect()
}

/// Loads each enabled target's stored ping samples once at boot (F1, Task 7
/// fix wave), so the Ping monitor can show real history before Start.
fn load_ping_history_task(settings: &Settings) -> Task<Message> {
    let names: Vec<String> = enabled_targets(settings)
        .into_iter()
        .map(|target| target.name.clone())
        .collect();
    Task::perform(load_ping_history(names), Message::PingHistoryLoaded)
}

async fn load_ping_history(names: Vec<String>) -> Vec<(String, Vec<PingSample>)> {
    let Ok(store) = open_store() else {
        return Vec::new();
    };
    names
        .into_iter()
        .filter_map(|name| {
            store
                .ping_sample_rtts_for(&name)
                .ok()
                .map(|rtts| (name, rtts_to_samples(rtts)))
        })
        .collect()
}

/// `ping_sample_rtts_for` returns bare `rtt_ms` values, oldest first, with no
/// timestamp column to rebuild `PingSample::at` from — this app's own stats,
/// sparkline and "is failing" logic only ever read `rtt`, never `at`, so
/// `SystemTime::now()` is a harmless placeholder here, not a real
/// measurement time.
fn rtts_to_samples(rtts: Vec<Option<f64>>) -> Vec<PingSample> {
    rtts.into_iter()
        .map(|rtt_ms| PingSample {
            at: SystemTime::now(),
            rtt: rtt_ms.map(|ms| Duration::from_secs_f64(ms / 1000.0)),
        })
        .collect()
}

/// `--scroll settings-notifications` (F4, Task 7 fix wave): snaps the
/// Settings screen's own scrollable to the bottom once at boot.
fn scroll_settings_to_notifications_task() -> Task<Message> {
    iced::advanced::widget::operate(iced::advanced::widget::operation::scrollable::snap_to::<
        Message,
    >(
        crate::screens::settings::body_scrollable_id(),
        iced::widget::scrollable::RelativeOffset::END.into(),
    ))
}

/// Shared consent gate for boot, About opening, and Check now.
fn wants_update_check(ui: &UiSettings) -> bool {
    ui.check_updates
}

/// One bounded GitHub request; preserve failures for the About result.
fn update_check_task() -> Task<Message> {
    Task::perform(
        async {
            let client = reqwest::Client::new();
            update::check(env!("CARGO_PKG_VERSION"), &client)
                .await
                .map_err(|error| error.to_string())
        },
        |result| match result {
            Ok(release) => Message::UpdateChecked(release),
            Err(error) => Message::UpdateCheckFailed(error),
        },
    )
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
    let work: Vec<(usize, String, Probe)> = targets
        .iter()
        .enumerate()
        .map(|(index, target)| (index, target.name.clone(), target.probe.clone()))
        .collect();
    Task::perform(ping_cycle(work), Message::TargetsRefreshed)
}

async fn ping_cycle(targets: Vec<(usize, String, Probe)>) -> Vec<(usize, String, PingSample)> {
    let started = Instant::now();
    let mut set = tokio::task::JoinSet::new();
    for (index, name, probe) in targets {
        set.spawn(async move { (index, name, probe_once(&probe, PING_INTERVAL).await) });
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

/// The store keys ping samples by target name (unaffected by C4: matching
/// live rows back to their index is an app-side concern, storage is not).
fn persist_ping_samples(samples: &[(usize, String, PingSample)]) {
    let Ok(store) = open_store() else {
        return;
    };
    for (_, name, sample) in samples {
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

fn load_latest_round(path: &std::path::Path) -> Result<Option<LatestRound>, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    let store = alidade_store::Store::open(path).map_err(|error| error.to_string())?;
    // The existing store API returns oldest first, with id as the tie breaker.
    // Read beyond the chart window so an old last round still appears at boot.
    let mut rows = store
        .rounds_between(SystemTime::UNIX_EPOCH, SystemTime::now())
        .map_err(|error| error.to_string())?;
    Ok(rows.pop().map(|row| LatestRound::Stored(Box::new(row))))
}

/// Same location the CLI writes to (`cli/src/main.rs::default_db_path`):
/// `%LOCALAPPDATA%\Alidade\alidade.db`, so a round the app measures shows up
/// in `alidade export` and vice versa. Not exported by `alidade-store` (the
/// CLI does not export its own copy either), so it is mirrored here rather
/// than reaching into a sibling crate's binary.
///
/// `ALIDADE_DB_PATH`, set from the `--db` startup flag (Task 7 pre-work B1),
/// names the exact file to open — unlike `ALIDADE_DATA_DIR` below it is not
/// joined with `Alidade/alidade.db`, since a screenshot run needs a literal,
/// caller-chosen path (e.g. a demo DB in a scratch temp directory).
pub(crate) fn default_db_path() -> PathBuf {
    if let Some(path) = std::env::var_os("ALIDADE_DB_PATH") {
        return PathBuf::from(path);
    }
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

fn shortcut(key: iced::keyboard::Key, modifiers: iced::keyboard::Modifiers) -> Option<Message> {
    use iced::keyboard::{key::Named, Key, Modifiers};
    match (key, modifiers) {
        (Key::Named(Named::F1), mode) if mode.is_empty() => Some(Message::HelpRequested),
        (Key::Named(Named::Escape), _) => Some(Message::EscapePressed),
        (Key::Named(Named::Tab), mode) if mode.is_empty() || mode == Modifiers::SHIFT => {
            Some(Message::MoveFocus(mode.shift()))
        }
        (Key::Character(key), mode) if mode == Modifiers::CTRL && key.as_str() == "," => {
            Some(Message::OpenSettings)
        }
        (Key::Character(key), mode) if mode == Modifiers::CTRL && key.eq_ignore_ascii_case("z") => {
            Some(Message::UndoShortcutRequested)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an `App` entirely in memory (no settings/db files touched),
    /// for tests that only need to exercise `update`'s synchronous state
    /// changes, not the async tasks it returns.
    fn test_app() -> App {
        let settings = Settings::default();
        let scheduler = Scheduler::new(settings.clone());
        let (history_from, history_to) =
            crate::screens::history::default_history_range(SystemTime::now());
        App {
            screen: Screen::default(),
            settings,
            settings_path: Settings::default_path(),
            settings_error: None,
            round_running: false,
            last_round: None,
            last_round_error: None,
            scheduler,
            continuous_running: false,
            continuous_paused: false,
            time_range: TimeRange::default(),
            chart_rows: Vec::new(),
            history_from,
            history_to,
            history_rows: Vec::new(),
            ping_running: false,
            clock_armed: false,
            ping_targets: Vec::new(),
            toast: None,
            removed_target: None,
            removal_generation: 0,
            load_error: None,
            ui_settings: UiSettings::default(),
            interval_text: "60".to_string(),
            budget_gb_text: "5.0".to_string(),
            notify_ping_ms_text: "150".to_string(),
            notify_download_mbit_text: "10".to_string(),
            notify_loss_pct_text: "5".to_string(),
            target_port_text: Vec::new(),
            new_target: NewTargetForm::default(),
            tray: None,
            window_id: None,
            start_minimized: false,
            notify_state: NotifyState::default(),
            system_theme: iced::theme::Mode::None,
            available_update: None,
            help_open: false,
            help_query: String::new(),
            help_opener: None,
            about_open: false,
            update_checking: false,
            last_update_result: None,
            last_update_at: None,
        }
    }

    fn sample(rtt_ms: u64) -> PingSample {
        PingSample {
            at: SystemTime::now(),
            rtt: Some(Duration::from_millis(rtt_ms)),
        }
    }

    /// C2 (Task 4 verification): an unparseable From/To used to be dropped
    /// silently. Now it sets an error toast.
    #[test]
    fn an_invalid_history_range_sets_an_error_toast_instead_of_failing_silently() {
        let mut app = test_app();
        let _ = app.update(Message::HistoryFromChanged("not-a-date".to_string()));
        let toast = app.toast.expect("an invalid range must set a toast");
        assert!(!toast.ok);
        assert!(toast.message.contains("not-a-date") || toast.message.contains("YYYY-MM-DD"));
    }

    #[test]
    fn a_valid_history_range_does_not_set_an_error_toast() {
        let mut app = test_app();
        let _ = app.update(Message::HistoryFromChanged("2026-01-01".to_string()));
        assert!(app.toast.is_none());
    }

    /// C4 (Task 4 verification): two targets sharing a name must not
    /// collapse into one row. Keying by index (rather than name) is the
    /// fix; this proves it directly against a duplicate-name pair.
    #[test]
    fn duplicate_target_names_do_not_collapse_into_one_row() {
        let mut app = test_app();
        app.ping_targets = vec![
            TargetLive {
                name: "dup".to_string(),
                probe: Probe::Icmp {
                    host: "1.1.1.1".to_string(),
                },
                samples: Vec::new(),
            },
            TargetLive {
                name: "dup".to_string(),
                probe: Probe::Icmp {
                    host: "8.8.8.8".to_string(),
                },
                samples: Vec::new(),
            },
        ];

        let _ = app.on_targets_refreshed(vec![
            (0, "dup".to_string(), sample(10)),
            (1, "dup".to_string(), sample(90)),
        ]);

        assert_eq!(app.ping_targets[0].samples.len(), 1);
        assert_eq!(app.ping_targets[1].samples.len(), 1);
        assert_eq!(
            app.ping_targets[0].samples[0].rtt,
            Some(Duration::from_millis(10)),
            "row 0 must keep its own sample, not row 1's"
        );
        assert_eq!(
            app.ping_targets[1].samples[0].rtt,
            Some(Duration::from_millis(90)),
            "row 1 must keep its own sample, not row 0's"
        );
    }

    fn target(name: &str, enabled: bool) -> TargetSpec {
        TargetSpec {
            name: name.to_string(),
            probe: Probe::Icmp {
                host: "127.0.0.1".to_string(),
            },
            verified: true,
            enabled,
        }
    }

    /// F1 (Task 7 fix wave): the Ping monitor's target list comes straight
    /// from settings' enabled targets, with no `ping_running` check at all —
    /// so a row exists for every enabled target before Start is ever
    /// pressed, and a disabled target gets none.
    #[test]
    fn sync_ping_targets_lists_every_enabled_settings_target_before_start() {
        let settings = Settings {
            targets: vec![
                target("on-a", true),
                target("off", false),
                target("on-b", true),
            ],
            ..Settings::default()
        };
        let synced = sync_ping_targets(&[], &settings);
        let names: Vec<&str> = synced.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["on-a", "on-b"]);
        assert!(
            synced.iter().all(|t| t.samples.is_empty()),
            "no samples yet, but the row must still exist"
        );
    }

    /// F1: resyncing (a settings edit, or pressing Start) must not throw
    /// away samples a row is already showing for a target that is still
    /// enabled.
    #[test]
    fn sync_ping_targets_preserves_existing_samples_by_name() {
        let settings = Settings {
            targets: vec![target("on-a", true)],
            ..Settings::default()
        };
        let existing = vec![TargetLive {
            name: "on-a".to_string(),
            probe: Probe::Icmp {
                host: "127.0.0.1".to_string(),
            },
            samples: vec![sample(42)],
        }];
        let synced = sync_ping_targets(&existing, &settings);
        assert_eq!(synced.len(), 1);
        assert_eq!(
            synced[0].samples[0].rtt,
            Some(Duration::from_millis(42)),
            "history must survive a resync"
        );
    }

    /// Task 6 Step 1: the tray's "Pause continuous" item must leave the app
    /// in exactly the state the in-window Pause button does — proven by
    /// routing both through the same `Message::Pause`, not by duplicating
    /// the assignment.
    #[test]
    fn the_tray_pause_message_pauses_continuous_the_same_way_the_button_does() {
        let mut app = test_app();
        app.continuous_running = true;
        let _ = app.update(Message::Tray(TrayMessage::PauseContinuous));
        assert!(!app.continuous_running);
        assert!(app.continuous_paused);
    }

    /// Task 6 Step 1: the tray's "Run single test" item starts a round the
    /// same way the Home button does.
    #[test]
    fn the_tray_run_single_test_message_starts_a_round() {
        let mut app = test_app();
        let _ = app.update(Message::Tray(TrayMessage::RunSingleTest));
        assert!(app.round_running);
    }

    /// Task 6 Step 2: with no window id yet (the brief moment before boot's
    /// lookup resolves), showing the window must not panic.
    #[test]
    fn showing_the_window_before_its_id_is_known_does_nothing() {
        let app = test_app();
        assert!(app.window_id.is_none());
        let _ = app.restore_window();
    }

    /// Task 6 Step 2: `--minimized` hides the window exactly once, the
    /// first time its id becomes known, and never again on a later
    /// `WindowIdReady` (there is none in practice, but the flag must not
    /// stay armed).
    #[test]
    fn minimized_start_arms_exactly_once() {
        let mut app = test_app();
        app.start_minimized = true;
        let id = window::Id::unique();
        let _ = app.on_window_id_ready(Some(id));
        assert_eq!(app.window_id, Some(id));
        assert!(!app.start_minimized, "the flag must not stay armed");
    }

    // -----------------------------------------------------------------
    // Task 7 pre-work A3: every `ThemeMode` resolves to the expected
    // palette, and the Settings change path (`SettingField::Theme`) updates
    // the palette `App::palette()` returns — without touching a real
    // window or renderer.
    // -----------------------------------------------------------------

    #[test]
    fn each_theme_mode_resolves_to_the_expected_palette() {
        let mut app = test_app();

        app.ui_settings.theme = ThemeMode::Light;
        assert!(
            !app.palette().dark,
            "Light must resolve to the light palette"
        );

        app.ui_settings.theme = ThemeMode::Dark;
        assert!(app.palette().dark, "Dark must resolve to the dark palette");

        app.ui_settings.theme = ThemeMode::System;
        app.system_theme = iced::theme::Mode::Dark;
        assert!(app.palette().dark, "System must follow a dark OS report");

        app.system_theme = iced::theme::Mode::Light;
        assert!(!app.palette().dark, "System must follow a light OS report");

        app.system_theme = iced::theme::Mode::None;
        assert!(
            !app.palette().dark,
            "System with no OS answer yet must read as light, not panic or guess dark"
        );
    }

    #[test]
    fn changing_the_theme_setting_updates_the_resolved_palette_immediately() {
        let mut app = test_app();
        assert!(
            !app.palette().dark,
            "default System + no OS report yet starts light"
        );

        let _ = app.update(Message::SettingChanged(SettingField::Theme(
            ThemeMode::Dark,
        )));
        assert_eq!(app.ui_settings.theme, ThemeMode::Dark);
        assert!(
            app.palette().dark,
            "the very next render must see the dark palette, with no extra step"
        );

        let _ = app.update(Message::SettingChanged(SettingField::Theme(
            ThemeMode::Light,
        )));
        assert!(!app.palette().dark);
    }

    /// Plan 064 Task 2: `App::boot` only schedules the GitHub check when
    /// `check_updates` is on. The default `UiSettings` is opted out.
    #[test]
    fn wants_update_check_follows_only_the_persisted_flag() {
        let mut ui = UiSettings::default();
        assert!(
            !wants_update_check(&ui),
            "the default settings must be opted out"
        );

        ui.check_updates = true;
        assert!(
            wants_update_check(&ui),
            "turning the setting on must schedule the check"
        );

        ui.check_updates = false;
        assert!(
            !wants_update_check(&ui),
            "turning it back off must not schedule the check"
        );
    }
    include!("../tests/support/t21.rs");
    include!("../tests/support/t22.rs");
    include!("../tests/support/t23.rs");
}
