//! Application state: `App`, `Message`, `Screen`.
//!
//! Task 1 (`docs/superpowers/plans/2026-08-20-alidade-ui.md`) only needs
//! screen selection. The engine/store-backed fields named in the plan's
//! Task 1 interfaces line (the live scheduler handle, the last
//! `RoundResult`, the loaded history window) land in Tasks 2 and 4, once a
//! screen actually consumes them — the engine interface changed today
//! (task 7b: split cadence, round kinds, load-bounded ping-under-load
//! window), so reaching for `alidade_engine::{RoundResult, Settings}` here
//! would fork against a fresh interface before anything needs it.

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

#[derive(Debug, Clone, Copy)]
pub enum Message {
    ScreenSelected(Screen),
}

/// The application's only long-lived state (plan 052 UI architecture: "a
/// new `app` crate ... owns the only long-lived state"). Task 1 holds just
/// the active screen; Tasks 2-6 add settings, the scheduler handle and the
/// last round result.
pub struct App {
    pub screen: Screen,
}

impl App {
    pub fn boot() -> (App, Task<Message>) {
        (App { screen: Screen::default() }, Task::none())
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ScreenSelected(screen) => self.screen = screen,
        }
        Task::none()
    }
}
