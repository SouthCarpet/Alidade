//! Settings screen (plan Task 5): interval, metric selection, theme,
//! autostart, data budget, targets editor, notification thresholds.
//!
//! Two different files get written here, both atomically (temp file, then
//! rename):
//! - measurement settings (interval, metrics, targets, budget) are
//!   `alidade_engine::Settings`, which already has an atomic `save` and its
//!   own tests (`engine/src/config.rs`) — this screen only edits the struct
//!   and calls that `save`.
//! - UI-only preferences (theme, autostart, notification thresholds) are not
//!   measurement config, so they are not part of `engine::Settings` at all;
//!   this module owns a small sibling file (`ui-settings.toml`, next to the
//!   engine's `settings.toml`) with the same temp-file-then-rename recipe.
//!
//! Live app-wide theme switching (Task 7 pre-work A1): `App::palette`
//! resolves the active `Palette` from this screen's `theme` choice and every
//! screen module now takes it as a parameter instead of hardcoding
//! `Palette::light()`. This screen still only captures and persists the
//! choice — `App::palette` is where it actually takes effect.

use std::fmt;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use alidade_engine::{MetricSelection, Probe, TargetSpec, MIN_CADENCE};
use design_tokens::scale::{RADIUS, SPACE, TYPE};
use iced::widget::{
    button, checkbox, column, container, pick_list, row, scrollable, text, text_input, tooltip,
    Space,
};
use iced::{Background, Border, Center, Color, Element, Fill, Length};

use crate::state::{App, Message};
use crate::theme::Palette;
use crate::ui::{font_medium, font_regular};

use super::with_toast;

// ---------------------------------------------------------------------
// Data model this screen edits (measurement fields live on `Settings`
// itself and are imported above).
// ---------------------------------------------------------------------

/// One of the three metric checkboxes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricKind {
    Download,
    Upload,
    Ping,
}

/// One of the two target kinds the engine's [`Probe`] supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    Icmp,
    Tcp,
}

pub const TARGET_KINDS: [TargetKind; 2] = [TargetKind::Icmp, TargetKind::Tcp];

impl fmt::Display for TargetKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            TargetKind::Icmp => "ICMP",
            TargetKind::Tcp => "TCP",
        })
    }
}

/// One editable field on an existing target row, or on the pending "add a
/// target" form (both send this, distinguished by which `Message` carries
/// it — see `state::App::edit_target` / `state::App::new_target`).
#[derive(Debug, Clone)]
pub enum TargetField {
    Name(String),
    Kind(TargetKind),
    Host(String),
    Port(String),
}

/// One theme choice. System is the default; App::palette resolves the OS
/// theme for every screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

pub const THEME_MODES: [ThemeMode; 3] = [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System];

impl fmt::Display for ThemeMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ThemeMode::System => "System",
            ThemeMode::Light => "Light",
            ThemeMode::Dark => "Dark",
        })
    }
}

/// One notification threshold: whether it is armed, and the value it fires
/// past.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Threshold {
    pub enabled: bool,
    pub value: f64,
}

/// UI-only preferences, persisted next to `engine::Settings` but not part of
/// it (see the module doc comment).
#[derive(Debug, Clone, PartialEq)]
pub struct UiSettings {
    pub theme: ThemeMode,
    pub autostart: bool,
    /// Opt-in, default off. Every update trigger uses this saved consent.
    /// Older files without this key still load as false.
    pub check_updates: bool,
    pub notify_master: bool,
    pub notify_ping_ms: Threshold,
    pub notify_download_mbit: Threshold,
    pub notify_loss_pct: Threshold,
}

impl Default for UiSettings {
    fn default() -> Self {
        UiSettings {
            theme: ThemeMode::System,
            autostart: false,
            check_updates: false,
            notify_master: false,
            notify_ping_ms: Threshold {
                enabled: false,
                value: 150.0,
            },
            notify_download_mbit: Threshold {
                enabled: false,
                value: 10.0,
            },
            notify_loss_pct: Threshold {
                enabled: false,
                value: 5.0,
            },
        }
    }
}

impl UiSettings {
    /// Sibling file next to the engine's own settings file.
    pub fn path_beside(settings_path: &Path) -> PathBuf {
        settings_path.with_file_name("ui-settings.toml")
    }

    /// A missing or corrupt file falls back to defaults; corrupt is reported
    /// so it is not silently swallowed (same honesty rule as
    /// `App::settings_error` for the engine's own settings file), but is
    /// never overwritten — this mirrors `Settings::load_or_create`'s
    /// "a broken hand edit stays repairable" rule.
    pub fn load_or_default(path: &Path) -> (Self, Option<String>) {
        match fs::read_to_string(path) {
            Ok(content) => match Self::parse(&content) {
                Ok(settings) => (settings, None),
                Err(err) => (Self::default(), Some(format!("ui-settings: {err}"))),
            },
            Err(_) => (Self::default(), None),
        }
    }

    fn parse(input: &str) -> Result<Self, String> {
        let mut out = Self::default();
        for raw in input.lines() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .map(|(k, v)| (k.trim(), v.trim()))
                .ok_or_else(|| format!("expected key = value, got `{line}`"))?;
            match key {
                "theme" => {
                    out.theme = match unquote(value)?.as_str() {
                        "system" => ThemeMode::System,
                        "light" => ThemeMode::Light,
                        "dark" => ThemeMode::Dark,
                        other => return Err(format!("unknown theme `{other}`")),
                    }
                }
                "autostart" => out.autostart = parse_bool(value)?,
                "check_updates" => out.check_updates = parse_bool(value)?,
                "notify_master" => out.notify_master = parse_bool(value)?,
                "notify_ping_enabled" => out.notify_ping_ms.enabled = parse_bool(value)?,
                "notify_ping_ms" => out.notify_ping_ms.value = parse_f64(value)?,
                "notify_download_enabled" => out.notify_download_mbit.enabled = parse_bool(value)?,
                "notify_download_mbit" => out.notify_download_mbit.value = parse_f64(value)?,
                "notify_loss_enabled" => out.notify_loss_pct.enabled = parse_bool(value)?,
                "notify_loss_pct" => out.notify_loss_pct.value = parse_f64(value)?,
                other => return Err(format!("unknown key `{other}`")),
            }
        }
        Ok(out)
    }

    fn to_toml(&self) -> String {
        format!(
            "theme = \"{}\"\nautostart = {}\ncheck_updates = {}\nnotify_master = {}\nnotify_ping_enabled = {}\nnotify_ping_ms = {}\nnotify_download_enabled = {}\nnotify_download_mbit = {}\nnotify_loss_enabled = {}\nnotify_loss_pct = {}\n",
            match self.theme {
                ThemeMode::System => "system",
                ThemeMode::Light => "light",
                ThemeMode::Dark => "dark",
            },
            self.autostart,
            self.check_updates,
            self.notify_master,
            self.notify_ping_ms.enabled,
            self.notify_ping_ms.value,
            self.notify_download_mbit.enabled,
            self.notify_download_mbit.value,
            self.notify_loss_pct.enabled,
            self.notify_loss_pct.value,
        )
    }

    /// Atomic: write to a scratch file next to the destination, flush, then
    /// rename over it — the same recipe as `engine::Settings::save`, and for
    /// the same reason (a crash mid-write must never leave a truncated
    /// file).
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let mut scratch_name = path
            .file_name()
            .map(|name| name.to_os_string())
            .unwrap_or_else(|| "ui-settings.toml".into());
        scratch_name.push(".tmp");
        let scratch = path.with_file_name(scratch_name);
        let mut file = fs::File::create(&scratch).map_err(|err| err.to_string())?;
        file.write_all(self.to_toml().as_bytes())
            .map_err(|err| err.to_string())?;
        file.sync_all().map_err(|err| err.to_string())?;
        drop(file);
        fs::rename(&scratch, path).map_err(|err| err.to_string())?;
        Ok(())
    }
}

fn unquote(value: &str) -> Result<String, String> {
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        Ok(value[1..value.len() - 1].to_string())
    } else {
        Err(format!("expected a quoted string, got `{value}`"))
    }
}

fn parse_bool(value: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(format!("expected true/false, got `{other}`")),
    }
}

fn parse_f64(value: &str) -> Result<f64, String> {
    value
        .parse()
        .map_err(|_| format!("expected a number, got `{value}`"))
}

/// The pending "add a target" form. Not persisted until `TargetAdded`
/// validates and commits it.
#[derive(Debug, Clone)]
pub struct NewTargetForm {
    pub name: String,
    pub kind: TargetKind,
    pub host: String,
    pub port: String,
}

impl Default for NewTargetForm {
    fn default() -> Self {
        NewTargetForm {
            name: String::new(),
            kind: TargetKind::Icmp,
            host: String::new(),
            port: "443".to_string(),
        }
    }
}

impl NewTargetForm {
    pub fn apply(&mut self, field: TargetField) {
        match field {
            TargetField::Name(text) => self.name = text,
            TargetField::Kind(kind) => self.kind = kind,
            TargetField::Host(text) => self.host = text,
            TargetField::Port(text) => self.port = text,
        }
    }
}

const DEFAULT_TCP_PORT: u16 = 443;

/// The port text a row starts with: the real port for a TCP target, empty
/// for ICMP (which has none).
pub(crate) fn port_text_for(target: &TargetSpec) -> String {
    match &target.probe {
        Probe::TcpConnect { port, .. } => port.to_string(),
        Probe::Icmp { .. } => String::new(),
    }
}

pub(crate) fn probe_host(probe: &Probe) -> &str {
    match probe {
        Probe::Icmp { host } | Probe::TcpConnect { host, .. } => host,
    }
}

pub(crate) fn set_probe_host(probe: &mut Probe, host: String) {
    match probe {
        Probe::Icmp { host: h } | Probe::TcpConnect { host: h, .. } => *h = host,
    }
}

/// Build a probe for `kind` from a host and a raw port buffer. An
/// unparseable port falls back to [`DEFAULT_TCP_PORT`] rather than refusing
/// the kind switch outright — the user can still fix the port afterwards.
pub(crate) fn build_probe(kind: TargetKind, host: String, port_text: &str) -> Probe {
    match kind {
        TargetKind::Icmp => Probe::Icmp { host },
        TargetKind::Tcp => Probe::TcpConnect {
            host,
            port: port_text.trim().parse().unwrap_or(DEFAULT_TCP_PORT),
        },
    }
}

/// Validate the pending "add a target" form into a real [`TargetSpec`]. A
/// user-added target is not community-sourced preset data, so it is not
/// marked `unverified` — that badge is specifically for the shipped
/// game-server presets (see `raw_speedtest_targets_2026-08.md`).
pub(crate) fn validate_new_target(form: &NewTargetForm) -> Result<TargetSpec, String> {
    let name = form.name.trim();
    if name.is_empty() {
        return Err("target name cannot be empty".to_string());
    }
    let host = form.host.trim();
    if host.is_empty() {
        return Err("target host cannot be empty".to_string());
    }
    let probe = match form.kind {
        TargetKind::Icmp => Probe::Icmp {
            host: host.to_string(),
        },
        TargetKind::Tcp => {
            let port: u16 = form.port.trim().parse().map_err(|_| {
                format!("port must be a number from 1 to 65535, got `{}`", form.port)
            })?;
            if port == 0 {
                return Err("port must be a number from 1 to 65535, got `0`".to_string());
            }
            Probe::TcpConnect {
                host: host.to_string(),
                port,
            }
        }
    };
    Ok(TargetSpec {
        name: name.to_string(),
        probe,
        verified: true,
        enabled: true,
    })
}

// ---------------------------------------------------------------------
// Pure validators (unit-tested below): the settings form's own rules.
// ---------------------------------------------------------------------

/// Interval text -> a cadence, enforcing the engine's own [`MIN_CADENCE`]
/// floor (60 s = 1 minute) so the UI never accepts a cadence the scheduler
/// would refuse to load later.
pub(crate) fn parse_interval_minutes(text: &str) -> Result<std::time::Duration, String> {
    let minutes: u64 = text
        .trim()
        .parse()
        .map_err(|_| format!("interval must be a whole number of minutes, got `{text}`"))?;
    let duration = std::time::Duration::from_secs(minutes.saturating_mul(60));
    if duration < MIN_CADENCE {
        return Err(format!(
            "interval must be at least {} minute",
            MIN_CADENCE.as_secs() / 60
        ));
    }
    Ok(duration)
}

/// Toggle one metric checkbox. Refuses to leave all three unchecked by
/// returning `None` instead of the updated selection.
pub(crate) fn toggle_metric(
    current: MetricSelection,
    kind: MetricKind,
    checked: bool,
) -> Option<MetricSelection> {
    let mut next = current;
    match kind {
        MetricKind::Download => next.download = checked,
        MetricKind::Upload => next.upload = checked,
        MetricKind::Ping => next.ping = checked,
    }
    if next.download || next.upload || next.ping {
        Some(next)
    } else {
        None
    }
}

pub(crate) fn parse_gb_to_bytes(text: &str) -> Result<u64, String> {
    let gb: f64 = text
        .trim()
        .parse()
        .map_err(|_| format!("budget must be a number of GB, got `{text}`"))?;
    if gb <= 0.0 {
        return Err("budget must be greater than zero".to_string());
    }
    Ok((gb * 1_000_000_000.0).round() as u64)
}

pub(crate) fn bytes_to_gb_text(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / 1_000_000_000.0)
}

/// What of today's budget has already been used, or `None` when budgeting is
/// off. Pure so the "today's usage" line is testable without a real clock.
pub(crate) fn today_usage_gb(
    daily_budget_bytes: Option<u64>,
    budget_left: Option<u64>,
) -> Option<f64> {
    let (budget, left) = (daily_budget_bytes?, budget_left?);
    Some(budget.saturating_sub(left) as f64 / 1_000_000_000.0)
}

// ---------------------------------------------------------------------
// View
// ---------------------------------------------------------------------

/// Id of this screen's own scrollable body. `--scroll settings-notifications`
/// (F4, plan 052 Task 7 fix wave) snaps it to the bottom once at boot, so a
/// screenshot taken right after launch can prove the Notifications block —
/// which sits below the fold at 1280x800 — is reachable, without guessing at
/// whether a tighter vertical rhythm alone would fit every row on screen.
pub fn body_scrollable_id() -> iced::widget::Id {
    iced::widget::Id::new("settings-body")
}

// Task 7 pre-work A1: `palette` now comes from the caller instead of a
// hardcoded `Palette::light()`.
pub fn view(app: &App, palette: Palette) -> Element<'_, Message> {
    let body = column![
        section_title(palette, "Measurement"),
        interval_row(palette, app),
        metrics_row(palette, app),
        autostart_row(palette, app),
        budget_section(palette, app),
        section_title(palette, "Targets"),
        targets_table(palette, app),
        add_target_row(palette, app),
        section_title(palette, "Notifications"),
        notifications_section(palette, app),
        section_title(palette, "Appearance"),
        theme_row(palette, app),
        about_section(palette, app),
    ]
    .spacing(SPACE.s6)
    .width(Fill);

    with_toast(
        palette,
        app,
        scrollable(body)
            .id(body_scrollable_id())
            .width(Fill)
            .height(Fill)
            .direction(scrollable::Direction::Vertical(
                scrollable::Scrollbar::new().spacing(SPACE.s2),
            ))
            .into(),
    )
}

fn section_title<'a>(palette: Palette, label: &'static str) -> Element<'a, Message> {
    text(label)
        .size(TYPE.md)
        .font(font_medium())
        .color(palette.text())
        .into()
}

fn field_label<'a>(palette: Palette, label: &'static str) -> Element<'a, Message> {
    text(label)
        .size(TYPE.xs)
        .font(font_medium())
        .color(palette.muted())
        .into()
}

fn note<'a>(palette: Palette, text_value: String) -> Element<'a, Message> {
    text(text_value)
        .size(TYPE.xs)
        .font(font_regular())
        .color(palette.muted())
        .into()
}

fn interval_row(palette: Palette, app: &App) -> Element<'_, Message> {
    let invalid = parse_interval_minutes(&app.interval_text).is_err();
    let input = text_input("60", &app.interval_text)
        .size(TYPE.sm)
        .padding([SPACE.s2, SPACE.s3])
        .width(Length::Fixed(96.0))
        .style(palette.text_input())
        .on_input(|text| Message::SettingChanged(SettingField::IntervalMinutes(text)));

    let mut column_items = vec![
        field_label(palette, "Throughput interval (minutes, min 1)"),
        row![input, text("minutes").size(TYPE.sm).color(palette.muted())]
            .spacing(SPACE.s2)
            .align_y(Center)
            .into(),
    ];
    if invalid {
        column_items.push(note(
            palette,
            "not saved yet: enter a whole number of minutes, at least 1".to_string(),
        ));
    }
    column(column_items).spacing(SPACE.s1).into()
}

fn metrics_row(palette: Palette, app: &App) -> Element<'_, Message> {
    let metrics = app.settings.metrics;
    column![
        field_label(palette, "Metrics (at least one stays selected)"),
        row![
            styled_checkbox(palette, "Download", metrics.download, |checked| {
                Message::SettingChanged(SettingField::Metric(MetricKind::Download, checked))
            }),
            styled_checkbox(palette, "Upload", metrics.upload, |checked| {
                Message::SettingChanged(SettingField::Metric(MetricKind::Upload, checked))
            }),
            styled_checkbox(palette, "Ping", metrics.ping, |checked| {
                Message::SettingChanged(SettingField::Metric(MetricKind::Ping, checked))
            }),
        ]
        .spacing(SPACE.s4),
    ]
    .spacing(SPACE.s1)
    .into()
}

fn theme_row(palette: Palette, app: &App) -> Element<'_, Message> {
    let segments = row(THEME_MODES.into_iter().map(|mode| {
        button(
            text(mode.to_string())
                .size(TYPE.sm)
                .font(if mode == app.ui_settings.theme {
                    font_medium()
                } else {
                    font_regular()
                }),
        )
        .padding([SPACE.s2, SPACE.s3])
        .style(palette.segmented(mode == app.ui_settings.theme))
        .on_press(Message::SettingChanged(SettingField::Theme(mode)))
        .into()
    }));
    column![field_label(palette, "Theme"), segments]
        .spacing(SPACE.s1)
        .into()
}

pub(crate) fn about_label(open: bool) -> &'static str {
    if open {
        "Hide About"
    } else {
        "Show About"
    }
}

fn about_section(palette: Palette, app: &App) -> Element<'_, Message> {
    let label = about_label(app.about_open);
    let heading = button(text(label).size(TYPE.md).font(font_medium()))
        .style(palette.btn_ghost())
        .on_press(Message::AboutToggled);
    let mut body = column![heading].spacing(SPACE.s3);
    if app.about_open {
        body = body.push(
            row![
                crate::ui::app_mark(palette),
                text(format!("Alidade {}", env!("CARGO_PKG_VERSION")))
                    .size(TYPE.md)
                    .font(font_medium())
                    .color(palette.text())
            ]
            .spacing(SPACE.s2)
            .align_y(Center),
        );
        body = body.push(
            text(app.update_status())
                .size(TYPE.sm)
                .color(palette.text()),
        );
        if let Some(at) = app.last_update_at {
            body = body.push(note(
                palette,
                format!("Last check: {} UTC (this session)", super::format_time(at)),
            ));
        }
        body = body.push(check_updates_row(palette, app));
        let check = button(text("Check now").size(TYPE.sm))
            .padding([SPACE.s2, SPACE.s3])
            .style(palette.btn_secondary());
        body = body.push(
            check.on_press_maybe(
                (app.ui_settings.check_updates && !app.update_checking)
                    .then_some(Message::CheckUpdatesNow),
            ),
        );
    }
    body.into()
}

fn autostart_row(palette: Palette, app: &App) -> Element<'_, Message> {
    styled_checkbox(
        palette,
        "Start Alidade with Windows",
        app.ui_settings.autostart,
        |checked| Message::SettingChanged(SettingField::Autostart(checked)),
    )
}

/// Plan 064 Task 2: opt-in, default off. The helper line says exactly what
/// the check does and does not do, so the checkbox needs no separate
/// confirmation dialog.
fn check_updates_row(palette: Palette, app: &App) -> Element<'_, Message> {
    column![
        styled_checkbox(palette, "Check for updates", app.ui_settings.check_updates,
            |checked| Message::SettingChanged(SettingField::CheckUpdates(checked))),
        note(palette, "When enabled: one GitHub request at start, when About opens, or on Check now. No download.".to_string()),
    ].spacing(SPACE.s1).into()
}

fn budget_section(palette: Palette, app: &App) -> Element<'_, Message> {
    let enabled = app.settings.daily_budget_bytes.is_some();
    let mut items = vec![styled_checkbox(
        palette,
        "Limit measurement traffic to a daily data budget",
        enabled,
        |checked| Message::SettingChanged(SettingField::BudgetEnabled(checked)),
    )];
    if enabled {
        let invalid = parse_gb_to_bytes(&app.budget_gb_text).is_err();
        let input = text_input("5.0", &app.budget_gb_text)
            .size(TYPE.sm)
            .padding([SPACE.s2, SPACE.s3])
            .width(Length::Fixed(96.0))
            .style(palette.text_input())
            .on_input(|text| Message::SettingChanged(SettingField::BudgetGb(text)));
        items.push(
            row![
                input,
                text("GB per day").size(TYPE.sm).color(palette.muted())
            ]
            .spacing(SPACE.s2)
            .align_y(Center)
            .into(),
        );
        if invalid {
            items.push(note(
                palette,
                "not saved yet: enter a GB value greater than zero".to_string(),
            ));
        }
        let usage = today_usage_gb(
            app.settings.daily_budget_bytes,
            app.scheduler.budget_left(std::time::SystemTime::now()),
        );
        if let Some(used_gb) = usage {
            items.push(note(palette, format!("used today: {used_gb:.2} GB")));
        }
    }
    column(items).spacing(SPACE.s2).into()
}

fn targets_table(palette: Palette, app: &App) -> Element<'_, Message> {
    let header = row![
        header_cell(palette, "Name", 3),
        header_cell(palette, "Kind", 2),
        header_cell(palette, "Host", 4),
        header_cell(palette, "Port", 2),
        header_cell(palette, "On", 1),
        header_cell(palette, "", 1),
    ]
    .spacing(SPACE.s2)
    .align_y(Center);

    let mut rows: Vec<Element<'_, Message>> = vec![header.into()];
    for (index, target) in app.settings.targets.iter().enumerate() {
        rows.push(target_row(palette, app, index, target));
    }
    palette.well_frame(column(rows).spacing(SPACE.s3).width(Fill))
}

fn header_cell<'a>(palette: Palette, label: &'static str, portion: u16) -> Element<'a, Message> {
    text(label)
        .size(TYPE.xs)
        .font(font_medium())
        .color(palette.muted())
        .width(Length::FillPortion(portion))
        .into()
}

fn target_row<'a>(
    palette: Palette,
    app: &'a App,
    index: usize,
    target: &'a TargetSpec,
) -> Element<'a, Message> {
    let kind = match target.probe {
        Probe::Icmp { .. } => TargetKind::Icmp,
        Probe::TcpConnect { .. } => TargetKind::Tcp,
    };
    let port_text = app
        .target_port_text
        .get(index)
        .cloned()
        .unwrap_or_else(|| port_text_for(target));

    let name_input = text_input("name", &target.name)
        .size(TYPE.sm)
        .padding([SPACE.s1, SPACE.s2])
        .width(Fill)
        .style(palette.text_input())
        .on_input(move |text| Message::TargetFieldChanged(index, TargetField::Name(text)));

    // The badge sits inside the Name column rather than as its own grid
    // cell, so an unverified row still has the same six columns as every
    // other row (a seventh, inserted cell would misalign the header).
    let name_cell: Element<'a, Message> = if target.verified {
        name_input.into()
    } else {
        row![name_input, unverified_badge(palette)]
            .spacing(SPACE.s1)
            .align_y(Center)
            .into()
    };

    let kind_pick = pick_list(TARGET_KINDS, Some(kind), move |kind| {
        Message::TargetFieldChanged(index, TargetField::Kind(kind))
    })
    .text_size(TYPE.sm)
    .padding([SPACE.s1, SPACE.s2])
    .width(Length::FillPortion(2))
    .style(pick_list_style(palette));

    let host_input = text_input("host or IP", probe_host(&target.probe))
        .size(TYPE.sm)
        .padding([SPACE.s1, SPACE.s2])
        .width(Length::FillPortion(4))
        .style(palette.text_input())
        .on_input(move |text| Message::TargetFieldChanged(index, TargetField::Host(text)));

    let port_input: Element<'a, Message> = if kind == TargetKind::Tcp {
        text_input("443", &port_text)
            .size(TYPE.sm)
            .padding([SPACE.s1, SPACE.s2])
            .width(Length::FillPortion(2))
            .style(palette.text_input())
            .on_input(move |text| Message::TargetFieldChanged(index, TargetField::Port(text)))
            .into()
    } else {
        Space::new().width(Length::FillPortion(2)).into()
    };

    let enabled_checkbox = container(
        checkbox(target.enabled)
            .size(18.0)
            .style(checkbox_style(palette))
            .on_toggle(move |_| Message::TargetToggled(index)),
    )
    .width(Length::FillPortion(1));

    let remove = button(text("Remove").size(TYPE.xs).font(font_medium()))
        .padding([SPACE.s1, SPACE.s2])
        .style(palette.btn_danger())
        .on_press(Message::TargetRemoved(index));
    let action_cell: Element<'a, Message> = container(remove).width(Length::FillPortion(1)).into();

    let name_cell_wrapped: Element<'a, Message> =
        container(name_cell).width(Length::FillPortion(3)).into();

    row![
        name_cell_wrapped,
        kind_pick,
        host_input,
        port_input,
        enabled_checkbox,
        action_cell,
    ]
    .spacing(SPACE.s2)
    .align_y(Center)
    .into()
}

/// A game-server preset that is community-sourced data, not measured by
/// this project (`rule_design_language.md`-adjacent honesty rule, same
/// spirit as `home.rs`'s "never a fake number").
fn unverified_badge<'a>(palette: Palette) -> Element<'a, Message> {
    let background: Color = palette.theme.warning_soft.into();
    let foreground: Color = palette.theme.warning.into();
    let badge = container(
        text("unverified")
            .size(TYPE.xs)
            .font(font_medium())
            .color(foreground),
    )
    .padding([SPACE.s1, SPACE.s2])
    .style(move |_| {
        container::Style::default()
            .background(background)
            .border(Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: iced::border::radius(RADIUS.pill),
            })
    });

    tooltip(
        badge,
        container(
            text("Community-sourced game-server target, not measured by this project. See vault/knowledge/raw/raw_speedtest_targets_2026-08.md.")
                .size(TYPE.xs)
                .color(palette.text()),
        )
        .padding(SPACE.s2)
        .style(move |_| container::Style::default().background(palette.raised()).border(Border {
            color: palette.border(),
            width: 1.0,
            radius: iced::border::radius(RADIUS.md),
        })),
        tooltip::Position::Top,
    )
    .into()
}

fn add_target_row(palette: Palette, app: &App) -> Element<'_, Message> {
    let form = &app.new_target;
    let name = text_input("name", &form.name)
        .size(TYPE.sm)
        .padding([SPACE.s1, SPACE.s2])
        .width(Length::FillPortion(3))
        .style(palette.text_input())
        .on_input(|text| Message::NewTargetFieldChanged(TargetField::Name(text)));

    let kind_pick = pick_list(TARGET_KINDS, Some(form.kind), |kind| {
        Message::NewTargetFieldChanged(TargetField::Kind(kind))
    })
    .text_size(TYPE.sm)
    .padding([SPACE.s1, SPACE.s2])
    .width(Length::FillPortion(2))
    .style(pick_list_style(palette));

    let host = text_input("host or IP", &form.host)
        .size(TYPE.sm)
        .padding([SPACE.s1, SPACE.s2])
        .width(Length::FillPortion(4))
        .style(palette.text_input())
        .on_input(|text| Message::NewTargetFieldChanged(TargetField::Host(text)));

    let port: Element<'_, Message> = if form.kind == TargetKind::Tcp {
        text_input("443", &form.port)
            .size(TYPE.sm)
            .padding([SPACE.s1, SPACE.s2])
            .width(Length::FillPortion(2))
            .style(palette.text_input())
            .on_input(|text| Message::NewTargetFieldChanged(TargetField::Port(text)))
            .into()
    } else {
        Space::new().width(Length::FillPortion(2)).into()
    };

    let add = button(text("Add target").size(TYPE.sm).font(font_medium()))
        .padding([SPACE.s2, SPACE.s4])
        .style(palette.btn_secondary())
        .on_press(Message::TargetAdded);

    row![name, kind_pick, host, port, add]
        .spacing(SPACE.s2)
        .align_y(Center)
        .into()
}

fn notifications_section(palette: Palette, app: &App) -> Element<'_, Message> {
    let ui = &app.ui_settings;
    column![
        styled_checkbox(
            palette,
            "Notify on threshold breaches",
            ui.notify_master,
            |checked| { Message::SettingChanged(SettingField::NotifyMaster(checked)) }
        ),
        threshold_row(
            palette,
            "Ping above",
            "ms",
            ui.notify_ping_ms.enabled,
            &app.notify_ping_ms_text,
            |checked| Message::SettingChanged(SettingField::NotifyPingEnabled(checked)),
            |text| Message::SettingChanged(SettingField::NotifyPingMs(text)),
        ),
        threshold_row(
            palette,
            "Download below",
            "Mbit/s",
            ui.notify_download_mbit.enabled,
            &app.notify_download_mbit_text,
            |checked| Message::SettingChanged(SettingField::NotifyDownloadEnabled(checked)),
            |text| Message::SettingChanged(SettingField::NotifyDownloadMbit(text)),
        ),
        threshold_row(
            palette,
            "Loss above",
            "%",
            ui.notify_loss_pct.enabled,
            &app.notify_loss_pct_text,
            |checked| Message::SettingChanged(SettingField::NotifyLossEnabled(checked)),
            |text| Message::SettingChanged(SettingField::NotifyLossPct(text)),
        ),
        note(
            palette,
            "Notifications are Windows toasts and are rate-limited to a few per hour.".to_string(),
        ),
    ]
    .spacing(SPACE.s2)
    .into()
}

fn threshold_row<'a>(
    palette: Palette,
    label: &'static str,
    unit: &'static str,
    enabled: bool,
    text_value: &'a str,
    on_toggle: impl Fn(bool) -> Message + 'a,
    on_input: impl Fn(String) -> Message + 'a,
) -> Element<'a, Message> {
    row![
        styled_checkbox(palette, label, enabled, on_toggle),
        text_input("", text_value)
            .size(TYPE.sm)
            .padding([SPACE.s1, SPACE.s2])
            .width(Length::Fixed(80.0))
            .style(palette.text_input())
            .on_input(on_input),
        text(unit).size(TYPE.sm).color(palette.muted()),
    ]
    .spacing(SPACE.s2)
    .align_y(Center)
    .into()
}

/// Checkbox style copied from `design/gallery-iced/src/main.rs`'s
/// `static_checkbox` (rule point 3: "copy its style functions rather than
/// inventing new ones"). Not in `theme.rs` because that file's edits are
/// scoped to focus geometry only for this task.
fn checkbox_style(palette: Palette) -> impl Fn(&iced::Theme, checkbox::Status) -> checkbox::Style {
    let t = *palette.theme;
    move |_theme, status| {
        let checked = matches!(
            status,
            checkbox::Status::Active { is_checked: true }
                | checkbox::Status::Hovered { is_checked: true }
        );
        checkbox::Style {
            background: Background::Color(if checked {
                t.accent.into()
            } else {
                t.surface_raised.into()
            }),
            icon_color: t.on_accent.into(),
            border: Border {
                color: if checked {
                    t.accent.into()
                } else {
                    t.border.into()
                },
                width: 1.0,
                radius: iced::border::radius(RADIUS.sm),
            },
            text_color: Some(t.text.into()),
        }
    }
}

fn styled_checkbox<'a>(
    palette: Palette,
    label: &'static str,
    checked: bool,
    on_toggle: impl Fn(bool) -> Message + 'a,
) -> Element<'a, Message> {
    checkbox(checked)
        .label(label)
        .size(18.0)
        .text_size(TYPE.sm)
        .style(checkbox_style(palette))
        .on_toggle(on_toggle)
        .into()
}

/// Pick-list style copied from the gallery's own `select` example the same
/// way `checkbox_style` copies `static_checkbox`.
fn pick_list_style(
    palette: Palette,
) -> impl Fn(&iced::Theme, iced::widget::pick_list::Status) -> iced::widget::pick_list::Style {
    let t = *palette.theme;
    move |_theme, _status| iced::widget::pick_list::Style {
        text_color: t.text.into(),
        placeholder_color: t.text_muted.into(),
        handle_color: t.text_muted.into(),
        background: Background::Color(t.surface_raised.into()),
        border: Border {
            color: t.border.into(),
            width: 1.0,
            radius: iced::border::radius(RADIUS.md),
        },
    }
}

/// One editable setting field. Extends the plan's `Message::SettingChanged`
/// interface (plan Task 5) rather than one message per field.
#[derive(Debug, Clone)]
pub enum SettingField {
    IntervalMinutes(String),
    Metric(MetricKind, bool),
    Theme(ThemeMode),
    Autostart(bool),
    CheckUpdates(bool),
    BudgetEnabled(bool),
    BudgetGb(String),
    NotifyMaster(bool),
    NotifyPingEnabled(bool),
    NotifyPingMs(String),
    NotifyDownloadEnabled(bool),
    NotifyDownloadMbit(String),
    NotifyLossEnabled(bool),
    NotifyLossPct(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn interval_below_one_minute_is_refused() {
        assert!(parse_interval_minutes("0").is_err());
        assert_eq!(
            parse_interval_minutes("1").unwrap(),
            Duration::from_secs(60)
        );
        assert_eq!(
            parse_interval_minutes("10").unwrap(),
            Duration::from_secs(600)
        );
    }

    #[test]
    fn interval_text_that_is_not_a_number_is_refused() {
        assert!(parse_interval_minutes("ten").is_err());
        assert!(parse_interval_minutes("").is_err());
    }

    #[test]
    fn unchecking_the_last_metric_is_refused() {
        let only_ping = MetricSelection {
            download: false,
            upload: false,
            ping: true,
        };
        assert_eq!(toggle_metric(only_ping, MetricKind::Ping, false), None);
    }

    #[test]
    fn unchecking_one_of_several_metrics_is_allowed() {
        let all = MetricSelection {
            download: true,
            upload: true,
            ping: true,
        };
        let next = toggle_metric(all, MetricKind::Upload, false).unwrap();
        assert!(next.download);
        assert!(!next.upload);
        assert!(next.ping);
    }

    #[test]
    fn checking_a_metric_back_on_is_allowed() {
        let ping_only = MetricSelection {
            download: false,
            upload: false,
            ping: true,
        };
        let next = toggle_metric(ping_only, MetricKind::Download, true).unwrap();
        assert!(next.download);
    }

    #[test]
    fn a_zero_or_negative_budget_is_refused() {
        assert!(parse_gb_to_bytes("0").is_err());
        assert!(parse_gb_to_bytes("-1").is_err());
        assert_eq!(parse_gb_to_bytes("5").unwrap(), 5_000_000_000);
    }

    #[test]
    fn budget_bytes_round_trip_through_gb_text() {
        assert_eq!(bytes_to_gb_text(5_000_000_000), "5.0");
        assert_eq!(
            parse_gb_to_bytes(&bytes_to_gb_text(5_000_000_000)).unwrap(),
            5_000_000_000
        );
    }

    #[test]
    fn today_usage_is_none_when_budgeting_is_off() {
        assert_eq!(today_usage_gb(None, None), None);
    }

    #[test]
    fn today_usage_is_the_budget_minus_what_is_left() {
        let usage = today_usage_gb(Some(5_000_000_000), Some(3_000_000_000)).unwrap();
        assert!((usage - 2.0).abs() < 1e-9);
    }

    #[test]
    fn a_new_target_needs_a_name_and_a_host() {
        let mut form = NewTargetForm::default();
        assert!(validate_new_target(&form).is_err(), "empty name and host");
        form.name = "Test".to_string();
        assert!(validate_new_target(&form).is_err(), "still no host");
        form.host = "1.2.3.4".to_string();
        let target = validate_new_target(&form).unwrap();
        assert_eq!(
            target.probe,
            Probe::Icmp {
                host: "1.2.3.4".to_string()
            }
        );
        assert!(
            target.verified,
            "a user-added target is not a community preset"
        );
    }

    #[test]
    fn a_tcp_target_needs_a_valid_port() {
        let form = NewTargetForm {
            name: "Test".to_string(),
            kind: TargetKind::Tcp,
            host: "1.2.3.4".to_string(),
            port: "not-a-port".to_string(),
        };
        assert!(validate_new_target(&form).is_err());

        let form = NewTargetForm {
            port: "443".to_string(),
            ..form
        };
        let target = validate_new_target(&form).unwrap();
        assert_eq!(
            target.probe,
            Probe::TcpConnect {
                host: "1.2.3.4".to_string(),
                port: 443
            }
        );
    }

    #[test]
    fn a_tcp_target_rejects_port_zero() {
        let form = NewTargetForm {
            name: "Test".to_string(),
            kind: TargetKind::Tcp,
            host: "1.2.3.4".to_string(),
            port: "0".to_string(),
        };
        assert!(validate_new_target(&form).is_err());
    }

    #[test]
    fn build_probe_falls_back_to_the_default_port_on_garbage_text() {
        let probe = build_probe(TargetKind::Tcp, "1.2.3.4".to_string(), "garbage");
        assert_eq!(
            probe,
            Probe::TcpConnect {
                host: "1.2.3.4".to_string(),
                port: DEFAULT_TCP_PORT
            }
        );
    }

    #[test]
    fn ui_settings_round_trips_through_toml() {
        let settings = UiSettings {
            theme: ThemeMode::Dark,
            autostart: true,
            check_updates: true,
            notify_master: true,
            notify_ping_ms: Threshold {
                enabled: true,
                value: 200.0,
            },
            notify_download_mbit: Threshold {
                enabled: false,
                value: 8.0,
            },
            notify_loss_pct: Threshold {
                enabled: true,
                value: 3.5,
            },
        };
        let text = settings.to_toml();
        let (parsed, error) = {
            match UiSettings::parse(&text) {
                Ok(parsed) => (parsed, None),
                Err(err) => (UiSettings::default(), Some(err)),
            }
        };
        assert_eq!(error, None, "round trip must parse cleanly");
        assert_eq!(parsed, settings);
    }

    /// Plan 064 Task 2: a file saved before `check_updates` existed has no
    /// such key. It must still parse, and load as opted out.
    #[test]
    fn a_ui_settings_file_without_check_updates_loads_as_opted_out() {
        let text = "theme = \"dark\"\nautostart = true\nnotify_master = false\n\
            notify_ping_enabled = false\nnotify_ping_ms = 150\n\
            notify_download_enabled = false\nnotify_download_mbit = 10\n\
            notify_loss_enabled = false\nnotify_loss_pct = 5\n";
        let parsed = UiSettings::parse(text).expect("an old file must still parse");
        assert!(
            !parsed.check_updates,
            "an old file must default to opted out"
        );
        assert!(
            parsed.autostart,
            "every other field still parses as written"
        );
    }

    fn scratch_dir() -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "alidade-ui-settings-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_missing_ui_settings_file_falls_back_to_defaults_without_an_error() {
        let dir = scratch_dir();
        let path = dir.join("ui-settings.toml");
        let (settings, error) = UiSettings::load_or_default(&path);
        assert_eq!(settings, UiSettings::default());
        assert_eq!(error, None);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_successful_save_replaces_the_file_and_leaves_no_scratch_behind() {
        let dir = scratch_dir();
        let path = dir.join("ui-settings.toml");
        fs::write(&path, "autostart = false\n").unwrap();

        let settings = UiSettings {
            autostart: true,
            ..UiSettings::default()
        };
        settings.save(&path).unwrap();

        let (loaded, error) = UiSettings::load_or_default(&path);
        assert_eq!(error, None);
        assert_eq!(loaded, settings);
        assert!(!path.with_file_name("ui-settings.toml.tmp").exists());
        fs::remove_dir_all(&dir).ok();
    }

    /// Same atomicity proof engine's `config.rs` uses: occupy the scratch
    /// path with a directory so the write cannot complete, and confirm the
    /// previous file survives untouched.
    #[test]
    fn a_save_that_cannot_complete_leaves_the_previous_file_intact() {
        let dir = scratch_dir();
        let path = dir.join("ui-settings.toml");
        let previous = "autostart = false\n";
        fs::write(&path, previous).unwrap();
        fs::create_dir(dir.join("ui-settings.toml.tmp")).unwrap();

        let result = UiSettings {
            autostart: true,
            ..UiSettings::default()
        }
        .save(&path);

        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), previous);
        fs::remove_dir_all(&dir).ok();
    }
}
