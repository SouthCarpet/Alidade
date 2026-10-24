//! The shell's view layer: the five-screen tab strip and the content area.
//!
//! Lives in the library rather than in `main.rs` because `App` is defined
//! here in the crate — an inherent `impl` for it cannot sit in the binary
//! target. `main.rs` keeps only the iced boot call.

use design_tokens::scale::{SPACE, TYPE};

use iced::widget::{button, column, container, row, space, text};
use iced::{Center, Color, Element, Fill, Font, Length};

use crate::state::{App, Message, Screen};
use crate::theme::Palette;
use crate::update;

pub fn font_regular() -> Font {
    design_tokens::fonts::regular()
}

pub fn font_medium() -> Font {
    Font {
        weight: iced::font::Weight::Medium,
        ..design_tokens::fonts::regular()
    }
}

/// The vendored number face has tabular figures and a plain zero.
/// iced cannot select OpenType features on an individual text run.
pub fn font_numeric() -> Font {
    design_tokens::fonts::numeric(iced::font::Weight::Normal)
}

impl App {
    /// Renders the five-screen tab strip and the active screen's content.
    /// Task 7 pre-work A1: the active palette is resolved exactly once here
    /// (`App::palette`, from `UiSettings.theme`) and handed to every screen
    /// — no screen resolves its own theme any more.
    pub fn view(&self) -> Element<'_, Message> {
        let palette = self.palette();

        let tabs = row(Screen::ALL
            .into_iter()
            .map(|screen| tab(palette, screen, self.screen)))
        .spacing(SPACE.s6);

        // Plan 064 Task 2: a quiet, right-aligned line next to the tabs when
        // a newer release is known. Nothing renders here otherwise.
        let header: Element<'_, Message> = match &self.available_update {
            Some(release) => row![tabs, space::horizontal(), update_link(palette, release)]
                .spacing(SPACE.s4)
                .width(Fill)
                .align_y(Center)
                .into(),
            None => tabs.into(),
        };

        let content: Element<'_, Message> = match self.screen {
            Screen::Home => crate::screens::home::view(self, palette),
            Screen::Continuous => crate::screens::continuous::view(self, palette),
            Screen::PingMonitor => crate::screens::ping_monitor::view(self, palette),
            Screen::History => crate::screens::history::view(self, palette),
            Screen::Settings => crate::screens::settings::view(self, palette),
        };

        let body = column![header, content]
            .spacing(SPACE.s6)
            .width(Fill)
            .height(Fill)
            .padding(SPACE.s6);

        container(body)
            .width(Fill)
            .height(Fill)
            .style(move |_| iced::widget::container::Style::default().background(palette.bg()))
            .into()
    }
}

/// One tab: label (medium weight + `text` colour when active, regular +
/// `muted` otherwise) over a 2px underline (`accent` when active,
/// transparent otherwise — same "always-present strip, colour toggles"
/// idiom as the gallery's `section_tabs`, so switching tabs never shifts
/// the row's height). Rendered as a ghost-style button so it is actually
/// clickable, unlike the gallery's static demo tabs.
fn tab(palette: Palette, screen: Screen, active: Screen) -> Element<'static, Message> {
    let is_active = screen == active;

    let label = text(screen.label())
        .size(TYPE.sm)
        .font(if is_active {
            font_medium()
        } else {
            font_regular()
        })
        .color(if is_active {
            palette.text()
        } else {
            palette.muted()
        });

    let underline = if is_active {
        strip(palette.accent(), 2.0)
    } else {
        strip(Color::TRANSPARENT, 2.0)
    };

    let content = column![label, underline].spacing(SPACE.s1);

    button(content)
        .padding([SPACE.s1, 0.0])
        .style(palette.btn_ghost())
        .on_press(Message::ScreenSelected(screen))
        .into()
}

/// Plan 064 Task 2: a ghost-styled link, not a call to action. Press opens
/// the release page in the browser (`Message::OpenReleasePage`); it never
/// downloads anything itself.
fn update_link<'a>(palette: Palette, release: &update::Release) -> Element<'a, Message> {
    button(
        text(format!("Dostupná je verzia {}", release.tag))
            .size(TYPE.xs)
            .font(font_regular())
            .color(palette.muted()),
    )
    .padding([SPACE.s1, SPACE.s2])
    .style(palette.btn_ghost())
    .on_press(Message::OpenReleasePage)
    .into()
}

fn strip(color: Color, height: f32) -> Element<'static, Message> {
    container(iced::widget::space::horizontal().height(Length::Fixed(height)))
        .width(Fill)
        .style(move |_| iced::widget::container::Style::default().background(color))
        .into()
}
