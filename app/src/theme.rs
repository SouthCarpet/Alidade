//! Kaliber style helpers, ported from `design/gallery-iced/src/main.rs`
//! (`rule_design_language.md` point 3: "the gallery is the reference").
//! Swapped `apps::pdf_editor` for `apps::alidade` (accent hue 190, petrol
//! teal, cool tint) — every colour, radius and duration below comes from a
//! `design_tokens::apps::alidade::{LIGHT, DARK}`/`scale` const, never a
//! literal (point 2: tokens are the only colours and times). This module
//! is the ONLY place in the app that reads `design_tokens`.
//!
//! Task 1's window shell and Task 2's Home screen between them now exercise
//! `bg`/`text`/`muted`/`accent`/`well`/`raised`/`card`/`btn_primary`/
//! `btn_secondary`/`btn_ghost`. `focus_ring` and `btn_danger` stay for
//! Tasks 4-6 (a settings/targets form needs both; Continuous/Settings need
//! a danger action) — the Task 1 interface contract those tasks build
//! screens against (see the plan's Task 1 "Interfaces" line), same
//! "defined for a later consumer" shape as `design-tokens`'s own
//! `schema.rs` dead-code allowance.

use std::time::Duration;

use design_tokens::apps::alidade::{DARK, LIGHT};
use design_tokens::scale::{MOTION, RADIUS, SPACE};
use design_tokens::Theme as TokenTheme;

use iced::widget::{button, container, text_input};
use iced::{Background, Border, Color, Element, Length, Shadow};

fn col(c: design_tokens::Rgba) -> Color {
    c.into()
}

fn radius(px: f32) -> iced::border::Radius {
    iced::border::radius(px)
}

/// The active light or dark token palette.
#[derive(Clone, Copy)]
pub struct Palette {
    pub theme: &'static TokenTheme,
    pub dark: bool,
}

impl Palette {
    pub fn light() -> Self {
        Palette {
            theme: &LIGHT,
            dark: false,
        }
    }

    pub fn dark() -> Self {
        Palette {
            theme: &DARK,
            dark: true,
        }
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

    pub fn danger(&self) -> Color {
        col(self.theme.danger)
    }

    pub fn success(&self) -> Color {
        col(self.theme.success)
    }

    pub fn border(&self) -> Color {
        col(self.theme.border)
    }

    /// Selected theme segment: sheet and hairline, shared selected fill.
    pub fn segmented(
        &self,
        selected: bool,
    ) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
        let palette = *self;
        move |theme, status| {
            let mut style = palette.btn_secondary()(theme, status);
            if selected && matches!(status, button::Status::Active) {
                style.background = Some(Background::Color(col(palette.theme.surface_selected)));
                style.text_color = palette.text();
            }
            style
        }
    }

    /// UI live-refresh interval from the motion scale. Used for the
    /// Continuous countdown, never as a measurement cadence.
    pub fn live_tick() -> Duration {
        Duration::from_millis(MOTION.live.round().max(1.0) as u64)
    }

    /// Recessed data well with a 1 px hairline and no shadow.
    pub fn well_frame<'a, Message: 'a>(
        &self,
        content: impl Into<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        let t = *self.theme;
        container(content)
            .padding(SPACE.s3)
            .width(Length::Fill)
            .style(move |_| design_tokens::styles::well_container(&t))
            .into()
    }

    /// C1 (Task 4 verification): focused text inputs now draw a real 2px
    /// ring in the focus colour, not a 1px colour swap. This is the closest
    /// approximation of "2px ring + 2px offset" that iced 0.14.2's `Style`
    /// API can express for this widget, not the full geometry — see the doc
    /// comment above [`Palette::focus_ring`] for exactly what is missing and
    /// why. It does not add a third style: focus is still only ever this one
    /// border, colour and width driven by `Status`, the same as before.
    pub fn text_input(&self) -> impl Fn(&iced::Theme, text_input::Status) -> text_input::Style {
        let t = *self.theme;
        move |_theme, status| {
            let (border_color, border_width) = match status {
                text_input::Status::Focused { .. } => (col(t.focus), 2.0),
                text_input::Status::Disabled => (col(t.disabled_fill), 1.0),
                text_input::Status::Hovered | text_input::Status::Active => (col(t.control), 1.0),
            };
            text_input::Style {
                background: Background::Color(col(t.surface_raised)),
                border: Border {
                    color: border_color,
                    width: border_width,
                    radius: radius(RADIUS.md),
                },
                icon: col(t.text_muted),
                placeholder: col(t.text_muted),
                value: col(t.text),
                selection: col(t.accent_soft),
            }
        }
    }

    /// "Focus = 2px ring + 2px offset" — one focus geometry everywhere
    /// (`rule_design_language.md` point 3(e)), ported verbatim from the
    /// gallery's `focus_ring`: a bordered container around the content,
    /// background left unset so the surrounding surface shows through the
    /// 2px offset gap.
    ///
    /// C1 (Task 4 verification) found this unused and `text_input` focus a
    /// 1px colour swap. `text_input`'s own style now draws a real 2px ring
    /// (see `Palette::text_input`), which is the closest this task gets to
    /// wiring this helper in for real, real OS focus, everywhere. Verified,
    /// not guessed: wiring THIS container around a widget so it reacts to
    /// that widget's own live focus would need iced to hand focus state to
    /// the PARENT, and iced 0.14.2 does not:
    /// - `text_input::Style` has no `shadow` field (only `background`,
    ///   `border`, `icon`, `placeholder`, `value`, `selection` —
    ///   `iced_widget-0.14.2/src/text_input.rs`), so there is no way to draw
    ///   a second, offset ring from inside the widget's own style either.
    /// - `button::Status` has no `Focused` variant at all in this version
    ///   (`Active`/`Hovered`/`Pressed`/`Disabled` only — confirmed by
    ///   grepping `iced_widget-0.14.2/src/button.rs` for "focus": zero
    ///   matches). Buttons have no keyboard-focus concept to react to here.
    ///
    /// Getting the full geometry on every focusable widget would need
    /// app-tracked focus (an iced `operation::focusable::find_focused`
    /// round trip re-issued after each relevant event) — real, but a much
    /// bigger change than this carry-over's severity warrants; left for a
    /// follow-up rather than guessed at. This helper stays exactly as it
    /// was so that follow-up has it ready to use.
    pub fn focus_ring<'a, Message: 'a>(
        &self,
        content: impl Into<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        let focus = col(self.theme.focus);
        container(content)
            .padding(2.0)
            .style(move |_| {
                container::Style::default().border(Border {
                    color: focus,
                    width: 2.0,
                    radius: radius(RADIUS.md + 2.0),
                })
            })
            .into()
    }

    /// Raised pane with a tone step and a 1 px hairline.
    pub fn card<'a, Message: 'a>(
        &self,
        content: impl Into<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        let t = *self.theme;
        container(content)
            .padding(SPACE.s4)
            .width(Length::Fill)
            .style(move |_| design_tokens::styles::card_container(&t))
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
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: radius(RADIUS.md),
                },
                shadow: Shadow::default(),
                snap: true,
            }
        }
    }

    pub fn btn_secondary(&self) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
        let t = *self.theme;
        move |_theme, status| {
            let (background, border_color, text_color) = match status {
                button::Status::Active => (col(t.surface_raised), col(t.control), col(t.text)),
                button::Status::Hovered => (col(t.surface_hover), col(t.control), col(t.text)),
                button::Status::Pressed => (col(t.well), col(t.control), col(t.text)),
                button::Status::Disabled => (
                    col(t.disabled_fill),
                    col(t.disabled_fill),
                    col(t.text_muted),
                ),
            };
            button::Style {
                background: Some(Background::from(background)),
                text_color,
                border: Border {
                    color: border_color,
                    width: 1.0,
                    radius: radius(RADIUS.md),
                },
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
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: radius(RADIUS.md),
                },
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
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: radius(RADIUS.md),
                },
                shadow: Shadow::default(),
                snap: true,
            }
        }
    }
}
