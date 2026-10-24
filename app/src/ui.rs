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

        let switch_label = if palette.dark {
            "Use light theme"
        } else {
            "Use dark theme"
        };
        let mut tools = row![
            space::horizontal(),
            button(text(switch_label).size(TYPE.xs))
                .padding([SPACE.s2, SPACE.s3])
                .style(palette.btn_ghost())
                .on_press(Message::QuickThemeSwitch)
        ]
        .align_y(Center)
        .spacing(SPACE.s2);
        if let Some(release) = &self.available_update {
            tools = tools.push(update_link(palette, release));
        }
        let brand = row![
            app_mark(palette),
            text("Alidade")
                .size(TYPE.md)
                .font(font_medium())
                .color(palette.text())
        ]
        .spacing(SPACE.s2)
        .align_y(Center);
        let header =
            column![row![brand, tools].width(Fill).align_y(Center), tabs].spacing(SPACE.s4);

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

/// Alidade mark geometry from assets/icon.svg, drawn in the active palette.
pub fn app_mark(palette: Palette) -> Element<'static, Message> {
    use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path};
    struct Mark(Palette);
    impl canvas::Program<Message> for Mark {
        type State = ();
        fn draw(
            &self,
            _: &(),
            renderer: &iced::Renderer,
            _: &iced::Theme,
            bounds: iced::Rectangle,
            _: iced::mouse::Cursor,
        ) -> Vec<Geometry> {
            let mut frame = Frame::new(renderer, bounds.size());
            // Use the same mark geometry as assets/icon.svg, with palette colours.
            let scale = bounds.width / 256.0;
            frame.scale(scale);
            let path = Path::new(|p| {
                p.move_to([64.0, 96.0].into());
                p.line_to([64.0, 116.0].into());
                p.line_to([192.0, 116.0].into());
                p.line_to([192.0, 72.0].into());
                p.move_to([60.0, 163.0].into());
                p.line_to([196.0, 163.0].into());
                p.move_to([76.0, 163.0].into());
                p.line_to([76.0, 187.0].into());
                p.move_to([128.0, 163.0].into());
                p.line_to([128.0, 194.0].into());
                p.move_to([180.0, 163.0].into());
                p.line_to([180.0, 187.0].into());
            });
            frame.stroke(
                &path,
                canvas::Stroke::default()
                    .with_color(self.0.accent())
                    .with_width(20.0),
            );
            frame.fill(&Path::circle([192.0, 46.0].into(), 20.0), self.0.accent());
            vec![frame.into_geometry()]
        }
    }
    Canvas::new(Mark(palette)).width(28.0).height(28.0).into()
}
