//! The shell's view layer: the five-screen tab strip and the content area.
//!
//! Lives in the library rather than in `main.rs` because `App` is defined
//! here in the crate — an inherent `impl` for it cannot sit in the binary
//! target. `main.rs` keeps only the iced boot call.

use design_tokens::scale::{SPACE, TYPE};

use iced::widget::{button, column, container, row, text, Space};
use iced::{Color, Element, Fill, Font, Length};

use crate::state::{App, Message, Screen};
use crate::theme::Palette;

pub fn font_regular() -> Font {
    Font::with_name("Inter")
}

pub fn font_medium() -> Font {
    Font { weight: iced::font::Weight::Medium, ..Font::with_name("Inter") }
}

/// Tabular-numeral fallback (same Tier-2 gap as the gallery's own
/// `font_numeric`, see `design/gallery-iced/src/main.rs`'s module doc):
/// `iced_core::Text` has no OpenType feature switch, so there is no way to
/// ask cosmic-text for `tnum`/`lnum` on Inter. A monospace digit run is
/// tabular by construction, so every KPI/table digit run uses this instead
/// of `font_regular`/`font_medium` — never the label next to it.
pub fn font_numeric() -> Font {
    Font::MONOSPACE
}

impl App {
    /// Renders the five-screen tab strip and the active screen's content.
    /// Home has real content as of Task 2; the other four still render
    /// Task 1's empty placeholder card until Tasks 4-5 build them.
    pub fn view(&self) -> Element<'_, Message> {
        // Theme switching is Task 5's job (the Settings screen); this task
        // hardcodes light.
        let palette = Palette::light();

        let tabs = row(Screen::ALL.into_iter().map(|screen| tab(palette, screen, self.screen))).spacing(SPACE.s6);

        let content: Element<'_, Message> = match self.screen {
            Screen::Home => crate::screens::home::view(self),
            _ => palette.card(false, Space::new().width(Fill).height(Fill)),
        };

        let body = column![tabs, content].spacing(SPACE.s6).width(Fill).height(Fill).padding(SPACE.s6);

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
        .font(if is_active { font_medium() } else { font_regular() })
        .color(if is_active { palette.text() } else { palette.muted() });

    let underline = if is_active { strip(palette.accent(), 2.0) } else { strip(Color::TRANSPARENT, 2.0) };

    let content = column![label, underline].spacing(SPACE.s1);

    button(content)
        .padding([SPACE.s1, 0.0])
        .style(palette.btn_ghost())
        .on_press(Message::ScreenSelected(screen))
        .into()
}

fn strip(color: Color, height: f32) -> Element<'static, Message> {
    container(iced::widget::space::horizontal().height(Length::Fixed(height)))
        .width(Fill)
        .style(move |_| iced::widget::container::Style::default().background(color))
        .into()
}
