//! Kaliber style helpers, ported from `design/gallery-iced/src/main.rs`
//! (`rule_design_language.md` point 3: "the gallery is the reference").
//! Swapped `apps::pdf_editor` for `apps::alidade` (accent hue 190, petrol
//! teal, cool tint) — every colour, radius and duration below comes from a
//! `design_tokens::apps::alidade::{LIGHT, DARK}`/`scale` const, never a
//! literal (point 2: tokens are the only colours and times). This module
//! is the ONLY place in the app that reads `design_tokens`.
//!
//! This task's window shell (the tab strip + an empty content area) only
//! exercises `bg`/`text`/`muted`/`accent`/`card`/`btn_ghost`. The rest of
//! this surface (`well`, `raised`, `focus_ring`, `btn_primary`,
//! `btn_secondary`, `btn_danger`) is the Task 1 interface contract Tasks
//! 2-6 build screens against (see the plan's Task 1 "Interfaces" line) —
//! unused here by design, same "defined for a later consumer" shape as
//! `design-tokens`'s own `schema.rs` dead-code allowance.
#![allow(dead_code)]

use design_tokens::apps::alidade::{DARK, LIGHT};
use design_tokens::scale::{RADIUS, SHADOW_RAISE_DARK, SHADOW_RAISE_LIGHT, SPACE};
use design_tokens::Theme as TokenTheme;

use iced::widget::{button, container, space};
use iced::{Background, Border, Color, Element, Length, Shadow, Vector};

fn col(c: design_tokens::Rgba) -> Color {
    c.into()
}

fn radius(px: f32) -> iced::border::Radius {
    iced::border::radius(px)
}

/// The active theme (light or dark) plus which mode it renders. `dark`
/// picks the milled-edge top-strip colour in [`Palette::card`]
/// (`border_top` in light, `milled` in dark) — the same role `Ctx::dark`
/// plays in the gallery.
#[derive(Clone, Copy)]
pub struct Palette {
    pub theme: &'static TokenTheme,
    pub dark: bool,
}

impl Palette {
    pub fn light() -> Self {
        Palette { theme: &LIGHT, dark: false }
    }

    pub fn dark() -> Self {
        Palette { theme: &DARK, dark: true }
    }

    pub fn bg(&self) -> Color {
        col(self.theme.background)
    }

    pub fn well(&self) -> Color {
        col(self.theme.well)
    }

    pub fn raised(&self) -> Color {
        col(self.theme.surface_raised)
    }

    pub fn text(&self) -> Color {
        col(self.theme.text)
    }

    pub fn muted(&self) -> Color {
        col(self.theme.text_muted)
    }

    pub fn accent(&self) -> Color {
        col(self.theme.accent)
    }

    /// "Focus = 2px ring + 2px offset" — one focus geometry everywhere
    /// (`rule_design_language.md` point 3(e)), ported verbatim from the
    /// gallery's `focus_ring`: a bordered container around the content,
    /// background left unset so the surrounding surface shows through the
    /// 2px offset gap.
    pub fn focus_ring<'a, Message: 'a>(&self, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
        let focus = col(self.theme.focus);
        container(content)
            .padding(2.0)
            .style(move |_| {
                container::Style::default()
                    .border(Border { color: focus, width: 2.0, radius: radius(RADIUS.md + 2.0) })
            })
            .into()
    }

    /// The Kaliber raised card: `surface_raised` background, `shadow-raise`,
    /// 1px `border`. When `milled` is true, a 2px top strip is prepended
    /// inside the same padded/bordered/shadowed container —
    /// `border_top` in light (the real per-side override colour), `milled`
    /// in dark (canon's "+0.04 inner highlight", alpha-blended live by the
    /// renderer against `surface_raised`) — identical geometry and colour
    /// rule to the gallery's `milled()` (see its doc comment for the
    /// per-side-border trade-off `iced_core::Border` forces).
    pub fn card<'a, Message: 'a>(
        &self,
        milled: bool,
        content: impl Into<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        let t = *self.theme;
        let shadow = if self.dark { SHADOW_RAISE_DARK } else { SHADOW_RAISE_LIGHT };
        let content = content.into();

        let inner: Element<'a, Message> = if milled {
            let strip_color = if self.dark { col(t.milled) } else { col(t.border_top) };
            let strip: Element<'a, Message> = container(space::horizontal().height(Length::Fixed(2.0)))
                .width(Length::Fill)
                .style(move |_| container::Style::default().background(strip_color))
                .into();
            iced::widget::column![strip, content].spacing(SPACE.s3).into()
        } else {
            content
        };

        container(inner)
            .padding(SPACE.s4)
            .width(Length::Fill)
            .style(move |_| {
                container::Style::default()
                    .background(col(t.surface_raised))
                    .border(Border { color: col(t.border), width: 1.0, radius: radius(RADIUS.lg) })
                    .shadow(Shadow {
                        color: col(shadow.color),
                        offset: Vector::new(shadow.x, shadow.y),
                        blur_radius: shadow.blur,
                    })
            })
            .into()
    }

    pub fn btn_primary(&self) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
        let t = *self.theme;
        move |_theme, status| {
            let (background, text_color) = match status {
                button::Status::Active => (col(t.accent), col(t.on_accent)),
                button::Status::Hovered => (col(t.accent_hover), col(t.on_accent)),
                button::Status::Pressed => (col(t.accent_active), col(t.on_accent)),
                button::Status::Disabled => (col(t.disabled_fill), col(t.text_muted)),
            };
            button::Style {
                background: Some(Background::from(background)),
                text_color,
                border: Border { color: Color::TRANSPARENT, width: 0.0, radius: radius(RADIUS.md) },
                shadow: Shadow::default(),
                snap: true,
            }
        }
    }

    pub fn btn_secondary(&self) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
        let t = *self.theme;
        move |_theme, status| {
            let (background, border_color, text_color) = match status {
                button::Status::Active => (col(t.surface_raised), col(t.border), col(t.text)),
                button::Status::Hovered => (col(t.surface_hover), col(t.border), col(t.text)),
                button::Status::Pressed => (col(t.well), col(t.border), col(t.text)),
                button::Status::Disabled => (col(t.disabled_fill), col(t.disabled_fill), col(t.text_muted)),
            };
            button::Style {
                background: Some(Background::from(background)),
                text_color,
                border: Border { color: border_color, width: 1.0, radius: radius(RADIUS.md) },
                shadow: Shadow::default(),
                snap: true,
            }
        }
    }

    pub fn btn_ghost(&self) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
        let t = *self.theme;
        move |_theme, status| {
            let (background, text_color) = match status {
                button::Status::Active => (None, col(t.text)),
                button::Status::Hovered => (Some(col(t.surface_hover)), col(t.text)),
                button::Status::Pressed => (Some(col(t.well)), col(t.text)),
                button::Status::Disabled => (None, col(t.text_muted)),
            };
            button::Style {
                background: background.map(Background::from),
                text_color,
                border: Border { color: Color::TRANSPARENT, width: 0.0, radius: radius(RADIUS.md) },
                shadow: Shadow::default(),
                snap: true,
            }
        }
    }

    pub fn btn_danger(&self) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
        let t = *self.theme;
        move |_theme, status| {
            let (background, text_color) = match status {
                button::Status::Active => (col(t.danger), col(t.on_accent)),
                button::Status::Hovered => (col(t.danger_hover), col(t.on_accent)),
                button::Status::Pressed => (col(t.danger_active), col(t.on_accent)),
                button::Status::Disabled => (col(t.disabled_fill), col(t.text_muted)),
            };
            button::Style {
                background: Some(Background::from(background)),
                text_color,
                border: Border { color: Color::TRANSPARENT, width: 0.0, radius: radius(RADIUS.md) },
                shadow: Shadow::default(),
                snap: true,
            }
        }
    }
}
