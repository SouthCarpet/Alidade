//! Shared iced 0.14 component styles (plan 094 Task 11, item J), rewritten
//! for direction A, "Register" (plan 094 revision 2). Moved from
//! `gallery-iced/src/main.rs`'s style closures so both the iced gallery and
//! Alidade (a vendored consumer) share one implementation instead of two.
//! Every colour comes from a resolved [`crate::Theme`] -- no literal here,
//! same rule the web `kaliber.css` follows.
//!
//! `button_ghost`/`button_secondary` share their hover/active colours,
//! same as the web `.k-btn-ghost`/`.k-btn-secondary` rule (components.md).
//! `iced_widget::button::Status` has no `Focused` variant (Tier-2 register
//! row, principles.md): the focus ring stays a caller-composed wrapper
//! (`focus_ring` in `gallery-iced/src/main.rs`), not part of this style.
//! Kaliber has no shadow token, so no style here draws a shadow.

use crate::Theme;
use iced_core::{Background, Border, Color, Shadow};
use iced_widget::{
    button, checkbox, container, overlay, pane_grid, pick_list, progress_bar, radio, slider,
    text_editor, text_input, toggler,
};

fn radius(px: f32) -> iced_core::border::Radius {
    iced_core::border::radius(px)
}

fn button_style(
    background: Option<Color>,
    border_color: Option<Color>,
    text_color: Color,
    radius_px: f32,
) -> button::Style {
    button::Style {
        background: background.map(Background::from),
        text_color,
        border: Border {
            color: border_color.unwrap_or(Color::TRANSPARENT),
            width: if border_color.is_some() { 1.0 } else { 0.0 },
            radius: radius(radius_px),
        },
        shadow: Shadow::default(),
        snap: true,
    }
}

/// `--color-accent` / `-hover` / `-active` / `disabled-fill` (components.md
/// "Buttons", Primary row).
pub fn button_primary(theme: &Theme, status: button::Status) -> button::Style {
    let t = theme;
    let (bg, text): (Color, Color) = match status {
        button::Status::Active => (t.accent.into(), t.on_accent.into()),
        button::Status::Hovered => (t.accent_hover.into(), t.on_accent.into()),
        button::Status::Pressed => (t.accent_active.into(), t.on_accent.into()),
        button::Status::Disabled => (t.disabled_fill.into(), t.text_muted.into()),
    };
    button_style(Some(bg), None, text, crate::scale::RADIUS.md)
}

/// `surface-raised` + 1px `control` border (3:1); hover shifts to
/// `surface-hover`, active drops to `well` (components.md "Buttons",
/// Secondary row).
pub fn button_secondary(theme: &Theme, status: button::Status) -> button::Style {
    let t = theme;
    let (bg, border): (Color, Color) = match status {
        button::Status::Active => (t.surface_raised.into(), t.control.into()),
        button::Status::Hovered => (t.surface_hover.into(), t.control.into()),
        button::Status::Pressed => (t.well.into(), t.control.into()),
        button::Status::Disabled => (t.disabled_fill.into(), t.disabled_fill.into()),
    };
    let text = if matches!(status, button::Status::Disabled) {
        t.text_muted.into()
    } else {
        t.text.into()
    };
    button_style(Some(bg), Some(border), text, crate::scale::RADIUS.md)
}

/// No fill or border of its own; shares secondary's hover/active values
/// (components.md "Buttons", Ghost row).
pub fn button_ghost(theme: &Theme, status: button::Status) -> button::Style {
    let t = theme;
    let bg: Option<Color> = match status {
        button::Status::Active => None,
        button::Status::Hovered => Some(t.surface_hover.into()),
        button::Status::Pressed => Some(t.well.into()),
        button::Status::Disabled => None,
    };
    let text = if matches!(status, button::Status::Disabled) {
        t.text_muted.into()
    } else {
        t.text.into()
    };
    button_style(bg, None, text, crate::scale::RADIUS.md)
}

/// `--color-danger` / `-hover` / `-active` / `disabled-fill`
/// (components.md "Buttons", Danger row).
pub fn button_danger(theme: &Theme, status: button::Status) -> button::Style {
    let t = theme;
    let (bg, text): (Color, Color) = match status {
        button::Status::Active => (t.danger.into(), t.on_accent.into()),
        button::Status::Hovered => (t.danger_hover.into(), t.on_accent.into()),
        button::Status::Pressed => (t.danger_active.into(), t.on_accent.into()),
        button::Status::Disabled => (t.disabled_fill.into(), t.text_muted.into()),
    };
    button_style(Some(bg), None, text, crate::scale::RADIUS.md)
}

/// The raised pane: `surface_raised` fill and a 1 px `border`, no shadow
/// (direction A: depth is a tone step plus a hairline). Mirrors the web
/// `.k-milled`.
pub fn card_container(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(Color::from(theme.surface_raised))
        .border(Border {
            color: theme.border.into(),
            width: 1.0,
            radius: radius(crate::scale::RADIUS.lg),
        })
}

/// The recessed well: `well` fill + `border`, no inset shadow
/// (`iced_core::Shadow` is outer-only, so inset never becomes a Tier-2
/// requirement -- components.md "Well").
pub fn well_container(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(Color::from(theme.well))
        .border(Border {
            color: theme.border.into(),
            width: 1.0,
            radius: radius(crate::scale::RADIUS.md),
        })
}

/// The art well: a plain well surface (direction A). No hatching and no
/// gradient (catalog SLOP-DECOR-05): put a drawing to scale or the real image
/// inside it.
pub fn art_well(theme: &Theme) -> container::Style {
    well_container(theme)
}

/// The caption plate: well fill + border, same formula, paired with an art
/// well (components.md "Art well and caption plate").
pub fn plate(theme: &Theme) -> container::Style {
    well_container(theme)
}

/// A list row (the web `.k-list-item`): transparent, or the accent-derived
/// `surface_selected` fill when selected. The selected state is a fill step,
/// never an edge stripe; the caller adds the bold title and the square marker.
pub fn list_row(theme: &Theme, selected: bool) -> container::Style {
    if selected {
        container::Style::default().background(Color::from(theme.surface_selected))
    } else {
        container::Style::default()
    }
}

/// Text input: `surface-raised` fill, a 1 px `control` boundary (3:1),
/// `accent-soft` selection. Status-invariant (the gallery renders one static
/// state, like the web gallery's `.k-input` demo).
pub fn text_input_style(theme: &Theme) -> text_input::Style {
    let t = theme;
    text_input::Style {
        background: Background::Color(t.surface_raised.into()),
        border: Border {
            color: t.control.into(),
            width: 1.0,
            radius: radius(crate::scale::RADIUS.md),
        },
        icon: t.text_muted.into(),
        placeholder: t.text_muted.into(),
        value: t.text.into(),
        selection: t.accent_soft.into(),
    }
}

// ---------------------------------------------------------------------
// Control set (plan 094 revision 2, lane K4, spec R9): the iced forms of the
// web controls, each a static style of the native iced widget. They draw the
// same states as `kaliber.css` (off, on, hover, disabled) from the same
// tokens. iced has no transition, so none of the web motions exist here
// (Tier-2 rows "Control state transitions" and "Sliding and turning marks",
// principles.md). The focus ring stays the caller-composed wrapper, as for
// buttons: none of these widgets has a focused status.
// ---------------------------------------------------------------------

/// The switch (web `.k-switch`): off = a raised track, a 1 px `control`
/// boundary and a `control` thumb; on = an accent track and an `on_accent`
/// thumb; hover steps the track tone; disabled = `disabled_fill`, a `border`
/// hairline and a muted thumb. The web also draws a bar or a ring as a mark;
/// the iced toggler has no slot for one (Tier-2 row).
pub fn toggler_style(theme: &Theme, status: toggler::Status) -> toggler::Style {
    let t = theme;
    let (on, hovered, disabled) = match status {
        toggler::Status::Active { is_toggled } => (is_toggled, false, false),
        toggler::Status::Hovered { is_toggled } => (is_toggled, true, false),
        toggler::Status::Disabled { is_toggled } => (is_toggled, false, true),
    };
    let (track, edge, thumb): (Color, Color, Color) = if disabled {
        (t.disabled_fill.into(), t.border.into(), t.text_muted.into())
    } else if on {
        let fill: Color = if hovered {
            t.accent_hover.into()
        } else {
            t.accent.into()
        };
        (fill, fill, t.on_accent.into())
    } else {
        let fill: Color = if hovered {
            t.surface_hover.into()
        } else {
            t.surface_raised.into()
        };
        (fill, t.control.into(), t.control.into())
    };
    toggler::Style {
        background: Background::Color(track),
        background_border_width: 1.0,
        background_border_color: edge,
        foreground: Background::Color(thumb),
        foreground_border_width: 0.0,
        foreground_border_color: Color::TRANSPARENT,
        text_color: Some(if disabled {
            t.text_muted.into()
        } else {
            t.text.into()
        }),
        border_radius: None,
        padding_ratio: 0.14,
    }
}

/// The checkbox (web `.k-check`): off = raised with a `control` border, on =
/// the accent with an `on_accent` mark; hover steps the tone; disabled =
/// `disabled_fill` with a `border` hairline and a muted mark.
pub fn checkbox_style(theme: &Theme, status: checkbox::Status) -> checkbox::Style {
    let t = theme;
    let (checked, hovered, disabled) = match status {
        checkbox::Status::Active { is_checked } => (is_checked, false, false),
        checkbox::Status::Hovered { is_checked } => (is_checked, true, false),
        checkbox::Status::Disabled { is_checked } => (is_checked, false, true),
    };
    let (fill, edge, mark): (Color, Color, Color) = if disabled {
        (t.disabled_fill.into(), t.border.into(), t.text_muted.into())
    } else if checked {
        let fill: Color = if hovered {
            t.accent_hover.into()
        } else {
            t.accent.into()
        };
        (fill, fill, t.on_accent.into())
    } else {
        let fill: Color = if hovered {
            t.surface_hover.into()
        } else {
            t.surface_raised.into()
        };
        (fill, t.control.into(), t.on_accent.into())
    };
    checkbox::Style {
        background: Background::Color(fill),
        icon_color: mark,
        border: Border {
            color: edge,
            width: 1.0,
            radius: radius(crate::scale::RADIUS.sm),
        },
        text_color: Some(if disabled {
            t.text_muted.into()
        } else {
            t.text.into()
        }),
    }
}

/// The radio (web `.k-radio-row`): a raised circle with a `control` border,
/// an accent border and dot when selected; hover steps the fill tone. iced
/// 0.14's `radio::Status` has no disabled variant, so a disabled radio has no
/// style (Tier-2 row).
pub fn radio_style(theme: &Theme, status: radio::Status) -> radio::Style {
    let t = theme;
    let (selected, hovered) = match status {
        radio::Status::Active { is_selected } => (is_selected, false),
        radio::Status::Hovered { is_selected } => (is_selected, true),
    };
    let fill: Color = if hovered {
        t.surface_hover.into()
    } else {
        t.surface_raised.into()
    };
    let edge: Color = match (selected, hovered) {
        (true, true) => t.accent_hover.into(),
        (true, false) => t.accent.into(),
        (false, _) => t.control.into(),
    };
    radio::Style {
        background: Background::Color(fill),
        dot_color: t.accent.into(),
        border_width: 1.0,
        border_color: edge,
        text_color: Some(t.text.into()),
    }
}

/// The slider (web `.k-slider`): a 4 px rail, `accent` up to the handle and
/// `control` after it, and a square handle of `radius.md` with a 2 px accent
/// border on a raised fill (hover: the hover tone, dragged: the well).
pub fn slider_style(theme: &Theme, status: slider::Status) -> slider::Style {
    let t = theme;
    let handle_fill: Color = match status {
        slider::Status::Active => t.surface_raised.into(),
        slider::Status::Hovered => t.surface_hover.into(),
        slider::Status::Dragged => t.well.into(),
    };
    slider::Style {
        rail: slider::Rail {
            backgrounds: (
                Background::Color(t.accent.into()),
                Background::Color(t.control.into()),
            ),
            width: 4.0,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: radius(crate::scale::RADIUS.pill),
            },
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Rectangle {
                width: 18,
                border_radius: radius(crate::scale::RADIUS.md),
            },
            background: Background::Color(handle_fill),
            border_width: 2.0,
            border_color: t.accent.into(),
        },
    }
}

/// The dot form of the slider (web `.k-slider--dot`): the slider's own rail and colours with a
/// round handle of the same 18 px size (`HandleShape::Circle`). Only the shape differs.
pub fn slider_dot_style(theme: &Theme, status: slider::Status) -> slider::Style {
    let mut style = slider_style(theme, status);
    style.handle.shape = slider::HandleShape::Circle { radius: 9.0 };
    style
}

/// Determinate progress (web `.k-progress`): a `well` rail with a 1 px
/// `control` border and an `accent` bar, both with the pill radius.
pub fn progress_style(theme: &Theme) -> progress_bar::Style {
    let t = theme;
    progress_bar::Style {
        background: Background::Color(t.well.into()),
        bar: Background::Color(t.accent.into()),
        border: Border {
            color: t.control.into(),
            width: 1.0,
            radius: radius(crate::scale::RADIUS.pill),
        },
    }
}

/// The select (web `.k-select`): raised with a `control` boundary, the hover
/// tone on hover, an accent border while open.
pub fn pick_list_style(theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    let t = theme;
    let (fill, edge): (Color, Color) = match status {
        pick_list::Status::Active => (t.surface_raised.into(), t.control.into()),
        pick_list::Status::Hovered => (t.surface_hover.into(), t.control.into()),
        pick_list::Status::Opened { .. } => (t.surface_raised.into(), t.accent.into()),
    };
    pick_list::Style {
        text_color: t.text.into(),
        placeholder_color: t.text_muted.into(),
        handle_color: t.text.into(),
        background: Background::Color(fill),
        border: Border {
            color: edge,
            width: 1.0,
            radius: radius(crate::scale::RADIUS.md),
        },
    }
}

/// The list a select opens (the web menu look): raised, a `border_strong`
/// border, radius `lg`, and the selected option on the `surface_selected` fill.
pub fn pick_list_menu_style(theme: &Theme) -> overlay::menu::Style {
    let t = theme;
    overlay::menu::Style {
        background: Background::Color(t.surface_raised.into()),
        border: Border {
            color: t.border_strong.into(),
            width: 1.0,
            radius: radius(crate::scale::RADIUS.lg),
        },
        text_color: t.text.into(),
        selected_text_color: t.text.into(),
        selected_background: Background::Color(t.surface_selected.into()),
        shadow: Shadow::default(),
    }
}

// ---------------------------------------------------------------------
// Gap controls (plan 094 revision 2, lane K5): the iced forms of the web
// `kaliber.css` "gap controls" block. iced has a native widget for four of
// them (`text_editor`, `tooltip`, `combo_box`, `pane_grid`); each is styled here
// or in the gallery from the same tokens. The rest are composed from
// containers, text and buttons in the gallery (Tier-2 rows, principles.md).
// ---------------------------------------------------------------------

/// The textarea (web `.k-textarea`): the text field look on the native
/// `text_editor`. Hover steps the tone, focus takes the accent border (the ring
/// is the caller-composed wrapper), an invalid text has the `danger` frame (the
/// caller adds the word Error), disabled is `disabled_fill` with a `border`
/// hairline.
pub fn text_editor_style(
    theme: &Theme,
    status: text_editor::Status,
    invalid: bool,
) -> text_editor::Style {
    let t = theme;
    let (fill, edge, value): (Color, Color, Color) = match status {
        text_editor::Status::Active => (t.surface_raised.into(), t.control.into(), t.text.into()),
        text_editor::Status::Hovered => (t.surface_hover.into(), t.control.into(), t.text.into()),
        text_editor::Status::Focused { .. } => {
            (t.surface_raised.into(), t.accent.into(), t.text.into())
        }
        text_editor::Status::Disabled => {
            (t.disabled_fill.into(), t.border.into(), t.text_muted.into())
        }
    };
    let edge: Color = if invalid && !matches!(status, text_editor::Status::Disabled) {
        t.danger.into()
    } else {
        edge
    };
    text_editor::Style {
        background: Background::Color(fill),
        border: Border {
            color: edge,
            width: 1.0,
            radius: radius(crate::scale::RADIUS.md),
        },
        placeholder: t.text_muted.into(),
        value,
        selection: t.accent_soft.into(),
    }
}

/// The tooltip bubble (web `.k-tip`): the text colour as the fill and the page
/// colour as the text, radius `sm`, no shadow.
pub fn tooltip_container(theme: &Theme) -> container::Style {
    container::Style {
        text_color: Some(theme.background.into()),
        background: Some(Background::Color(theme.text.into())),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: radius(crate::scale::RADIUS.sm),
        },
        shadow: Shadow::default(),
        snap: true,
    }
}

/// The toggle button (web `.k-toggle`): a secondary-button look. Pressed
/// (`aria-pressed`) is the `surface_selected` fill and a full accent border; the
/// caller adds the filled square mark. Disabled is `disabled_fill`.
pub fn toggle_button_style(theme: &Theme, status: button::Status, pressed: bool) -> button::Style {
    let t = theme;
    let (fill, edge, text): (Color, Color, Color) = match (pressed, status) {
        (_, button::Status::Disabled) => {
            (t.disabled_fill.into(), t.border.into(), t.text_muted.into())
        }
        (_, button::Status::Pressed) => (
            t.well.into(),
            if pressed {
                t.accent.into()
            } else {
                t.control.into()
            },
            t.text.into(),
        ),
        (true, _) => (t.surface_selected.into(), t.accent.into(), t.text.into()),
        (false, button::Status::Hovered) => {
            (t.surface_hover.into(), t.control.into(), t.text.into())
        }
        (false, button::Status::Active) => {
            (t.surface_raised.into(), t.control.into(), t.text.into())
        }
    };
    button_style(Some(fill), Some(edge), text, crate::scale::RADIUS.md)
}

/// The resizable panels (web `.k-panels`): a hovered split is a `control` line,
/// a dragged one an accent line, and a hovered drop region the selected fill.
pub fn pane_grid_style(theme: &Theme) -> pane_grid::Style {
    let t = theme;
    pane_grid::Style {
        hovered_region: pane_grid::Highlight {
            background: Background::Color(t.surface_selected.into()),
            border: Border {
                color: t.accent.into(),
                width: 1.0,
                radius: radius(0.0),
            },
        },
        picked_split: pane_grid::Line {
            color: t.accent.into(),
            width: 2.0,
        },
        hovered_split: pane_grid::Line {
            color: t.control.into(),
            width: 2.0,
        },
    }
}

#[cfg(test)]
mod tests {
    use iced_widget::{button, checkbox, pane_grid, radio, slider, text_editor, toggler};

    /// Any generated app theme will do: the tests check how a style is built from a theme,
    /// not which app's colours it is, so a vendored build with one app file passes too.
    fn light() -> crate::Theme {
        *crate::APPS[0].1
    }

    fn dark() -> crate::Theme {
        *crate::APPS[0].2
    }

    #[test]
    fn switch_off_is_a_raised_track_with_a_control_boundary_and_on_is_the_accent() {
        let t = light();
        let off = super::toggler_style(&t, toggler::Status::Active { is_toggled: false });
        assert_eq!(
            off.background,
            iced_core::Background::Color(t.surface_raised.into())
        );
        assert_eq!(
            off.background_border_color,
            iced_core::Color::from(t.control)
        );
        assert_eq!(
            off.foreground,
            iced_core::Background::Color(t.control.into())
        );
        let on = super::toggler_style(&t, toggler::Status::Active { is_toggled: true });
        assert_eq!(on.background, iced_core::Background::Color(t.accent.into()));
        assert_eq!(
            on.foreground,
            iced_core::Background::Color(t.on_accent.into())
        );
        let hover = super::toggler_style(&t, toggler::Status::Hovered { is_toggled: true });
        assert_eq!(
            hover.background,
            iced_core::Background::Color(t.accent_hover.into())
        );
        let disabled = super::toggler_style(&t, toggler::Status::Disabled { is_toggled: true });
        assert_eq!(
            disabled.background,
            iced_core::Background::Color(t.disabled_fill.into())
        );
    }

    #[test]
    fn checkbox_checked_is_the_accent_and_disabled_is_the_disabled_fill() {
        let t = dark();
        let on = super::checkbox_style(&t, checkbox::Status::Active { is_checked: true });
        assert_eq!(on.background, iced_core::Background::Color(t.accent.into()));
        assert_eq!(on.icon_color, iced_core::Color::from(t.on_accent));
        let off = super::checkbox_style(&t, checkbox::Status::Active { is_checked: false });
        assert_eq!(off.border.color, iced_core::Color::from(t.control));
        let disabled = super::checkbox_style(&t, checkbox::Status::Disabled { is_checked: false });
        assert_eq!(
            disabled.background,
            iced_core::Background::Color(t.disabled_fill.into())
        );
    }

    #[test]
    fn radio_selected_takes_the_accent_border_and_hover_only_changes_the_fill() {
        let t = light();
        let selected = super::radio_style(&t, radio::Status::Active { is_selected: true });
        assert_eq!(selected.border_color, iced_core::Color::from(t.accent));
        assert_eq!(selected.dot_color, iced_core::Color::from(t.accent));
        let unselected = super::radio_style(&t, radio::Status::Active { is_selected: false });
        assert_eq!(unselected.border_color, iced_core::Color::from(t.control));
        let hovered = super::radio_style(&t, radio::Status::Hovered { is_selected: false });
        assert_eq!(
            hovered.background,
            iced_core::Background::Color(t.surface_hover.into())
        );
    }

    #[test]
    fn slider_rail_is_accent_then_control_and_the_handle_has_an_accent_border() {
        let t = light();
        let style = super::slider_style(&t, slider::Status::Active);
        assert_eq!(
            style.rail.backgrounds.0,
            iced_core::Background::Color(t.accent.into())
        );
        assert_eq!(
            style.rail.backgrounds.1,
            iced_core::Background::Color(t.control.into())
        );
        assert_eq!(style.rail.width, 4.0);
        assert_eq!(style.handle.border_color, iced_core::Color::from(t.accent));
        assert_eq!(
            super::slider_style(&t, slider::Status::Dragged)
                .handle
                .background,
            iced_core::Background::Color(t.well.into())
        );
    }

    #[test]
    fn the_dot_slider_is_the_slider_with_a_round_handle_and_the_same_colours() {
        let t = light();
        for status in [
            slider::Status::Active,
            slider::Status::Hovered,
            slider::Status::Dragged,
        ] {
            let square = super::slider_style(&t, status);
            let dot = super::slider_dot_style(&t, status);
            assert!(matches!(
                square.handle.shape,
                slider::HandleShape::Rectangle { .. }
            ));
            assert_eq!(
                dot.handle.shape,
                slider::HandleShape::Circle { radius: 9.0 }
            );
            assert_eq!(dot.handle.background, square.handle.background);
            assert_eq!(dot.handle.border_color, square.handle.border_color);
            assert_eq!(dot.rail.backgrounds, square.rail.backgrounds);
        }
    }

    #[test]
    fn progress_is_a_well_rail_with_an_accent_bar_and_a_control_border() {
        let t = light();
        let style = super::progress_style(&t);
        assert_eq!(
            style.background,
            iced_core::Background::Color(t.well.into())
        );
        assert_eq!(style.bar, iced_core::Background::Color(t.accent.into()));
        assert_eq!(style.border.color, iced_core::Color::from(t.control));
    }

    #[test]
    fn select_and_its_list_use_the_control_boundary_and_the_selected_fill() {
        let t = light();
        let select = super::pick_list_style(&t, iced_widget::pick_list::Status::Active);
        assert_eq!(select.border.color, iced_core::Color::from(t.control));
        let menu = super::pick_list_menu_style(&t);
        assert_eq!(
            menu.selected_background,
            iced_core::Background::Color(t.surface_selected.into())
        );
        assert_eq!(menu.shadow, iced_core::Shadow::default());
    }

    #[test]
    fn textarea_hover_focus_error_and_disabled_take_their_tokens() {
        let t = light();
        let rest = super::text_editor_style(&t, text_editor::Status::Active, false);
        assert_eq!(rest.border.color, iced_core::Color::from(t.control));
        let hover = super::text_editor_style(&t, text_editor::Status::Hovered, false);
        assert_eq!(
            hover.background,
            iced_core::Background::Color(t.surface_hover.into())
        );
        let focus = super::text_editor_style(
            &t,
            text_editor::Status::Focused { is_hovered: false },
            false,
        );
        assert_eq!(focus.border.color, iced_core::Color::from(t.accent));
        let error = super::text_editor_style(&t, text_editor::Status::Active, true);
        assert_eq!(error.border.color, iced_core::Color::from(t.danger));
        let off = super::text_editor_style(&t, text_editor::Status::Disabled, true);
        assert_eq!(
            off.background,
            iced_core::Background::Color(t.disabled_fill.into())
        );
        assert_eq!(off.border.color, iced_core::Color::from(t.border));
    }

    #[test]
    fn tooltip_bubble_is_the_inverse_of_the_page_and_has_no_shadow() {
        let t = dark();
        let style = super::tooltip_container(&t);
        assert_eq!(
            style.background,
            Some(iced_core::Background::Color(t.text.into()))
        );
        assert_eq!(style.text_color, Some(iced_core::Color::from(t.background)));
        assert_eq!(style.shadow, iced_core::Shadow::default());
    }

    #[test]
    fn toggle_button_pressed_is_a_fill_step_with_an_accent_frame() {
        let t = light();
        let off = super::toggle_button_style(&t, button::Status::Active, false);
        assert_eq!(off.border.color, iced_core::Color::from(t.control));
        let on = super::toggle_button_style(&t, button::Status::Active, true);
        assert_eq!(
            on.background,
            Some(iced_core::Background::Color(t.surface_selected.into()))
        );
        assert_eq!(on.border.color, iced_core::Color::from(t.accent));
        let off_disabled = super::toggle_button_style(&t, button::Status::Disabled, false);
        assert_eq!(
            off_disabled.background,
            Some(iced_core::Background::Color(t.disabled_fill.into()))
        );
    }

    #[test]
    fn pane_split_is_a_control_line_and_a_dragged_split_is_the_accent() {
        let t = light();
        let style = super::pane_grid_style(&t);
        assert_eq!(style.hovered_split.color, iced_core::Color::from(t.control));
        assert_eq!(style.picked_split.color, iced_core::Color::from(t.accent));
        let _: pane_grid::Style = style;
    }

    #[test]
    fn primary_button_active_returns_the_accent_fill() {
        let t = light();
        let style = super::button_primary(&t, button::Status::Active);
        assert_eq!(
            style.background,
            Some(iced_core::Background::Color(t.accent.into()))
        );
    }

    #[test]
    fn primary_button_disabled_returns_disabled_fill() {
        let t = light();
        let style = super::button_primary(&t, button::Status::Disabled);
        assert_eq!(
            style.background,
            Some(iced_core::Background::Color(t.disabled_fill.into()))
        );
    }

    #[test]
    fn ghost_button_shares_secondary_hover_and_active() {
        let t = light();
        let ghost_hover = super::button_ghost(&t, button::Status::Hovered);
        let secondary_hover = super::button_secondary(&t, button::Status::Hovered);
        assert_eq!(ghost_hover.background, secondary_hover.background);
        let ghost_active = super::button_ghost(&t, button::Status::Pressed);
        let secondary_active = super::button_secondary(&t, button::Status::Pressed);
        assert_eq!(ghost_active.background, secondary_active.background);
    }

    #[test]
    fn art_well_is_the_plain_well_surface_with_the_border() {
        let t = light();
        let style = super::art_well(&t);
        assert_eq!(
            style.background,
            Some(iced_core::Background::Color(t.well.into()))
        );
        assert_eq!(style.border.color, iced_core::Color::from(t.border));
        assert!(style.border.width > 0.0);
    }

    #[test]
    fn card_container_is_flat_with_no_shadow() {
        let t = light();
        let style = super::card_container(&t);
        assert_eq!(
            style.background,
            Some(iced_core::Background::Color(t.surface_raised.into()))
        );
        assert_eq!(style.shadow, iced_core::Shadow::default());
    }

    #[test]
    fn secondary_button_and_input_use_the_control_boundary() {
        let t = light();
        let button = super::button_secondary(&t, button::Status::Active);
        assert_eq!(button.border.color, iced_core::Color::from(t.control));
        assert_eq!(
            super::text_input_style(&t).border.color,
            iced_core::Color::from(t.control)
        );
    }

    #[test]
    fn selected_list_row_is_a_fill_step_and_an_unselected_row_has_none() {
        let t = light();
        let selected = super::list_row(&t, true);
        assert_eq!(
            selected.background,
            Some(iced_core::Background::Color(t.surface_selected.into()))
        );
        assert_eq!(
            selected.border.width, 0.0,
            "a selected row has no edge stripe"
        );
        assert_eq!(super::list_row(&t, false).background, None);
    }
}
