//! Startup flags for deterministic screenshot mode (plan 052, UI Task 7
//! pre-work B1).
//!
//! These only affect the very first boot — none of them are written back to
//! a settings file, so a screenshot run never leaves a trace in the real
//! user's configuration. Parsing lives here (the library target) rather than
//! in `main.rs` itself so it stays reachable from `app/tests/`, the same
//! "everything testable lives in the library" rule `lib.rs`'s module doc
//! states for `chart`/`update`.

use std::path::PathBuf;

use crate::screens::settings::ThemeMode;
use crate::state::Screen;

/// `--scroll <target>` (F4, plan 052 Task 7 fix wave): snaps a screen's own
/// scrollable to a known position once at boot, so a screenshot taken right
/// after launch can prove content that sits below the fold is reachable.
/// The only target today is the Settings screen's Notifications block,
/// which the default 1280x800 window does not have room for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollTarget {
    SettingsNotifications,
}

/// Parsed command-line flags. `--minimized` (Task 6) lives here too now,
/// rather than as a bare bool in `main.rs`, so every startup flag has one
/// parser and one test suite.
#[derive(Debug, Clone, PartialEq)]
pub struct StartupFlags {
    pub minimized: bool,
    /// `--screen <home|continuous|ping|history|settings>`.
    pub screen: Option<Screen>,
    /// `--theme <light|dark>`. Overrides the saved `ui_settings.theme` for
    /// this run only.
    pub theme: Option<ThemeMode>,
    /// `--window <WxH>`, e.g. `1280x800`.
    pub window: (f32, f32),
    /// `--db <path>`. Opens the store at this literal path instead of the
    /// default `%LOCALAPPDATA%\Alidade\alidade.db`.
    pub db: Option<PathBuf>,
    /// `--settings <path>` (F1, plan 052 Task 7 fix wave). Loads and saves
    /// `engine::Settings` at this literal path instead of the default
    /// `%APPDATA%\Alidade\settings.toml` — a demo DB and the real default
    /// settings file can otherwise name different targets, which is exactly
    /// what made the Ping monitor show none while Settings showed targets
    /// enabled.
    pub settings: Option<PathBuf>,
    /// `--scroll <target>`. See [`ScrollTarget`].
    pub scroll: Option<ScrollTarget>,
}

const DEFAULT_WINDOW: (f32, f32) = (1280.0, 800.0);

impl Default for StartupFlags {
    fn default() -> Self {
        StartupFlags {
            minimized: false,
            screen: None,
            theme: None,
            window: DEFAULT_WINDOW,
            db: None,
            settings: None,
            scroll: None,
        }
    }
}

/// Parses argv (already skipping `argv[0]`) into [`StartupFlags`]. An
/// unrecognised flag, or a value that fails to parse, is ignored rather than
/// panicking or aborting startup — a bad flag must never stop the app from
/// opening.
pub fn parse<I: Iterator<Item = String>>(args: I) -> StartupFlags {
    let mut flags = StartupFlags::default();
    let mut iter = args;
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--minimized" => flags.minimized = true,
            "--screen" => {
                if let Some(value) = iter.next() {
                    if let Some(screen) = parse_screen(&value) {
                        flags.screen = Some(screen);
                    }
                }
            }
            "--theme" => {
                if let Some(value) = iter.next() {
                    if let Some(theme) = parse_theme(&value) {
                        flags.theme = Some(theme);
                    }
                }
            }
            "--window" => {
                if let Some(value) = iter.next() {
                    if let Some(window) = parse_window(&value) {
                        flags.window = window;
                    }
                }
            }
            "--db" => {
                if let Some(value) = iter.next() {
                    flags.db = Some(PathBuf::from(value));
                }
            }
            "--settings" => {
                if let Some(value) = iter.next() {
                    flags.settings = Some(PathBuf::from(value));
                }
            }
            "--scroll" => {
                if let Some(value) = iter.next() {
                    if let Some(target) = parse_scroll(&value) {
                        flags.scroll = Some(target);
                    }
                }
            }
            _ => {}
        }
    }
    flags
}

fn parse_screen(value: &str) -> Option<Screen> {
    match value {
        "home" => Some(Screen::Home),
        "continuous" => Some(Screen::Continuous),
        "ping" => Some(Screen::PingMonitor),
        "history" => Some(Screen::History),
        "settings" => Some(Screen::Settings),
        _ => None,
    }
}

fn parse_theme(value: &str) -> Option<ThemeMode> {
    match value {
        "light" => Some(ThemeMode::Light),
        "dark" => Some(ThemeMode::Dark),
        _ => None,
    }
}

fn parse_scroll(value: &str) -> Option<ScrollTarget> {
    match value {
        "settings-notifications" => Some(ScrollTarget::SettingsNotifications),
        _ => None,
    }
}

/// `WxH` in whole pixels, e.g. `1280x800`.
fn parse_window(value: &str) -> Option<(f32, f32)> {
    let (w, h) = value.split_once('x')?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(args: &[&str]) -> StartupFlags {
        parse(args.iter().map(|a| a.to_string()))
    }

    #[test]
    fn no_flags_gives_all_defaults() {
        let flags = parse_str(&[]);
        assert_eq!(flags, StartupFlags::default());
    }

    #[test]
    fn minimized_sets_only_that_flag() {
        let flags = parse_str(&["--minimized"]);
        assert!(flags.minimized);
        assert_eq!(flags.screen, None);
    }

    #[test]
    fn screen_flag_parses_every_known_value() {
        assert_eq!(parse_str(&["--screen", "home"]).screen, Some(Screen::Home));
        assert_eq!(
            parse_str(&["--screen", "continuous"]).screen,
            Some(Screen::Continuous)
        );
        assert_eq!(
            parse_str(&["--screen", "ping"]).screen,
            Some(Screen::PingMonitor)
        );
        assert_eq!(
            parse_str(&["--screen", "history"]).screen,
            Some(Screen::History)
        );
        assert_eq!(
            parse_str(&["--screen", "settings"]).screen,
            Some(Screen::Settings)
        );
    }

    #[test]
    fn an_unknown_screen_value_is_ignored_not_a_panic() {
        let flags = parse_str(&["--screen", "nonsense"]);
        assert_eq!(flags.screen, None);
    }

    #[test]
    fn theme_flag_parses_light_and_dark() {
        assert_eq!(
            parse_str(&["--theme", "light"]).theme,
            Some(ThemeMode::Light)
        );
        assert_eq!(parse_str(&["--theme", "dark"]).theme, Some(ThemeMode::Dark));
        assert_eq!(parse_str(&["--theme", "system"]).theme, None);
    }

    #[test]
    fn window_flag_parses_width_and_height() {
        let flags = parse_str(&["--window", "1280x800"]);
        assert_eq!(flags.window, (1280.0, 800.0));
    }

    #[test]
    fn a_malformed_window_value_keeps_the_default() {
        let flags = parse_str(&["--window", "not-a-size"]);
        assert_eq!(flags.window, DEFAULT_WINDOW);
        let flags = parse_str(&["--window", "1280"]);
        assert_eq!(flags.window, DEFAULT_WINDOW);
    }

    #[test]
    fn db_flag_captures_the_literal_path() {
        let flags = parse_str(&["--db", "C:/temp/alidade-shoot/demo.db"]);
        assert_eq!(
            flags.db,
            Some(PathBuf::from("C:/temp/alidade-shoot/demo.db"))
        );
    }

    #[test]
    fn settings_flag_captures_the_literal_path() {
        let flags = parse_str(&["--settings", "C:/temp/alidade-shoot/settings.toml"]);
        assert_eq!(
            flags.settings,
            Some(PathBuf::from("C:/temp/alidade-shoot/settings.toml"))
        );
    }

    #[test]
    fn scroll_flag_parses_the_known_target() {
        let flags = parse_str(&["--scroll", "settings-notifications"]);
        assert_eq!(flags.scroll, Some(ScrollTarget::SettingsNotifications));
    }

    #[test]
    fn an_unknown_scroll_value_is_ignored_not_a_panic() {
        let flags = parse_str(&["--scroll", "nonsense"]);
        assert_eq!(flags.scroll, None);
    }

    #[test]
    fn flags_combine_in_any_order() {
        let flags = parse_str(&[
            "--theme",
            "dark",
            "--screen",
            "history",
            "--window",
            "1024x768",
            "--minimized",
        ]);
        assert!(flags.minimized);
        assert_eq!(flags.screen, Some(Screen::History));
        assert_eq!(flags.theme, Some(ThemeMode::Dark));
        assert_eq!(flags.window, (1024.0, 768.0));
    }

    #[test]
    fn a_dangling_flag_missing_its_value_is_ignored() {
        let flags = parse_str(&["--screen"]);
        assert_eq!(flags.screen, None);
    }
}
