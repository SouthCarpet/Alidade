//! System tray icon, menu, and the close-to-tray decision (plan 052, UI
//! Task 6).
//!
//! Building the tray icon and wiring `tray-icon`/`muda` is Windows-only
//! (Global Constraint: the tray path is `cfg(windows)`-gated, and the app
//! must still compile on other targets with it stubbed). The decision
//! function at the bottom carries no such gate — it is plain logic, tested
//! on every platform.

use iced::futures::channel::mpsc::UnboundedSender;

/// One event the tray can raise into the app's `update` loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayMessage {
    /// Restore the main window: the "Show Alidade" menu item, or a left
    /// click on the icon itself.
    ShowWindow,
    RunSingleTest,
    PauseContinuous,
    /// The only real exit — see [`should_hide_on_close`].
    Quit,
}

/// The tray failed to build (icon rejected, or the OS refused the
/// notification-area registration). The app still runs, windowed, without
/// one; [`should_hide_on_close`] turns close-to-tray off in that case so a
/// closed window is never unreachable.
#[derive(Debug)]
pub struct TrayError(String);

impl std::fmt::Display for TrayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "tray: {}", self.0)
    }
}

impl std::error::Error for TrayError {}

/// Keeps the tray icon and its menu alive. Dropping this removes the icon
/// from the notification area, so the caller must hold it for the life of
/// the app (a field on `App`), never as a local that goes out of scope at
/// the end of `boot`.
#[cfg(windows)]
pub struct TrayHandle {
    _icon: tray_icon::TrayIcon,
    _menu: tray_icon::menu::Menu,
}

#[cfg(not(windows))]
pub struct TrayHandle;

const MENU_SHOW: &str = "show";
const MENU_RUN_SINGLE: &str = "run-single";
const MENU_PAUSE: &str = "pause";
const MENU_QUIT: &str = "quit";

/// 32x32 raw RGBA pixels rasterised once from `assets/icon.svg` (vendored,
/// not generated at build time — the source SVG is a handful of static
/// shapes, so a one-off raster avoids adding an SVG-to-PNG build dependency
/// for a mark that never changes at runtime). Stored as raw pixels, not a
/// PNG, so `tray-icon` can use them directly with no image-decoding
/// dependency either.
const ICON_SIZE: u32 = 32;
#[cfg(windows)]
const ICON_RGBA: &[u8] = include_bytes!("../assets/icon-32.rgba");

/// Build the tray icon and menu, and wire both event sources (icon clicks,
/// menu selections) into `sender`. Every recognised event becomes one
/// [`TrayMessage`]; hover/move icon events and unknown menu ids are
/// dropped rather than forwarded — they have no message to become.
///
/// Must be called on the same thread that will run the window's event loop
/// (i.e. from `App::boot`, before `.run()` starts it) — `tray-icon`'s
/// hidden window shares that thread's message queue on Windows.
#[cfg(windows)]
pub fn spawn(sender: UnboundedSender<TrayMessage>) -> Result<TrayHandle, TrayError> {
    use tray_icon::menu::{Menu, MenuEvent, MenuItem};
    use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let icon = Icon::from_rgba(ICON_RGBA.to_vec(), ICON_SIZE, ICON_SIZE)
        .map_err(|err| TrayError(err.to_string()))?;

    let menu = Menu::new();
    let show = MenuItem::with_id(MENU_SHOW, "Show Alidade", true, None);
    let run_single = MenuItem::with_id(MENU_RUN_SINGLE, "Run single test", true, None);
    let pause = MenuItem::with_id(MENU_PAUSE, "Pause continuous", true, None);
    let quit = MenuItem::with_id(MENU_QUIT, "Quit", true, None);
    for item in [&show, &run_single, &pause, &quit] {
        menu.append(item)
            .map_err(|err| TrayError(err.to_string()))?;
    }

    let icon_handle = TrayIconBuilder::new()
        .with_icon(icon)
        .with_tooltip("Alidade")
        .with_menu(Box::new(menu.clone()))
        .build()
        .map_err(|err| TrayError(err.to_string()))?;

    let click_sender = sender.clone();
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            let _ = click_sender.unbounded_send(TrayMessage::ShowWindow);
        }
    }));

    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let message = if event.id() == MENU_SHOW {
            Some(TrayMessage::ShowWindow)
        } else if event.id() == MENU_RUN_SINGLE {
            Some(TrayMessage::RunSingleTest)
        } else if event.id() == MENU_PAUSE {
            Some(TrayMessage::PauseContinuous)
        } else if event.id() == MENU_QUIT {
            Some(TrayMessage::Quit)
        } else {
            None
        };
        if let Some(message) = message {
            let _ = sender.unbounded_send(message);
        }
    }));

    Ok(TrayHandle {
        _icon: icon_handle,
        _menu: menu,
    })
}

#[cfg(not(windows))]
pub fn spawn(_sender: UnboundedSender<TrayMessage>) -> Result<TrayHandle, TrayError> {
    Err(TrayError(
        "tray is Windows-only in this release".to_string(),
    ))
}

/// Whether a window close request should hide the window (close-to-tray)
/// instead of exiting the process outright.
///
/// Both conditions matter: hiding with no tray to restore from (a spawn
/// failure, or a non-Windows build) would trap the user with no way back to
/// the window, so that case always falls through to a real close; hiding an
/// idle window when nothing is measuring just delays the exit the user
/// asked for. Quit from the tray menu bypasses this decision entirely and
/// always exits for real.
pub fn should_hide_on_close(tray_available: bool, continuous_running: bool) -> bool {
    tray_available && continuous_running
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_only_when_both_a_tray_exists_and_continuous_mode_is_running() {
        assert!(should_hide_on_close(true, true));
    }

    #[test]
    fn a_missing_tray_always_lets_the_window_close_for_real() {
        assert!(!should_hide_on_close(false, true));
        assert!(!should_hide_on_close(false, false));
    }

    #[test]
    fn an_idle_window_with_a_tray_still_closes_normally() {
        assert!(!should_hide_on_close(true, false));
    }
}
