//! The contrast rules of the materials (plan 125, spec R25): text and muted text
//! at least 4.5:1 on paper (`background`), sheet (`surface_raised`), well, ink
//! and field (`accent_soft`, plus the second-hue field) in both themes; disabled
//! text at least 3:1. One list, read by `design-tokens contrast` and the tests.

use crate::{contrast_ratio, Accent2, Rgba, Theme};

/// Text and muted text: WCAG AA for body text.
pub const TEXT_MIN: f64 = 4.5;
/// Disabled text, a chart mark or a control boundary: WCAG 1.4.11.
pub const NON_TEXT_MIN: f64 = 3.0;

/// One checked pair: a name, the foreground, the background and the minimum.
pub type Pair = (&'static str, Rgba, Rgba, f64);

/// Every material pair of one theme. Add an `accent2` for an app with a second hue.
pub fn pairs(t: &Theme, accent2: Option<&Accent2>) -> Vec<Pair> {
    let mut out: Vec<Pair> = vec![
        ("text/paper", t.text, t.background, TEXT_MIN),
        ("text/sheet", t.text, t.surface_raised, TEXT_MIN),
        ("text/well", t.text, t.well, TEXT_MIN),
        ("text/field", t.text, t.accent_soft, TEXT_MIN),
        ("text/hover", t.text, t.surface_hover, TEXT_MIN),
        ("text/active", t.text, t.surface_active, TEXT_MIN),
        ("text/selected", t.text, t.surface_selected, TEXT_MIN),
        ("muted/paper", t.text_muted, t.background, TEXT_MIN),
        ("muted/sheet", t.text_muted, t.surface_raised, TEXT_MIN),
        ("muted/well", t.text_muted, t.well, TEXT_MIN),
        ("muted/field", t.text_muted, t.accent_soft, TEXT_MIN),
        ("muted/hover", t.text_muted, t.surface_hover, TEXT_MIN),
        ("muted/selected", t.text_muted, t.surface_selected, TEXT_MIN),
        ("on-ink/ink", t.on_ink, t.ink, TEXT_MIN),
        ("on-ink-muted/ink", t.on_ink_muted, t.ink, TEXT_MIN),
        (
            "on-ink-action/ink-action",
            t.on_ink_action,
            t.ink_action,
            TEXT_MIN,
        ),
        ("ink-action/ink", t.ink_action, t.ink, NON_TEXT_MIN),
        (
            "disabled/paper",
            t.disabled_text,
            t.background,
            NON_TEXT_MIN,
        ),
        (
            "disabled/sheet",
            t.disabled_text,
            t.surface_raised,
            NON_TEXT_MIN,
        ),
        ("disabled/well", t.disabled_text, t.well, NON_TEXT_MIN),
        (
            "disabled/field",
            t.disabled_text,
            t.accent_soft,
            NON_TEXT_MIN,
        ),
        (
            "disabled/disabled-fill",
            t.disabled_text,
            t.disabled_fill,
            NON_TEXT_MIN,
        ),
    ];
    if let Some(a2) = accent2 {
        out.extend([
            ("text/accent2-field", t.text, a2.field, TEXT_MIN),
            ("muted/accent2-field", t.text_muted, a2.field, TEXT_MIN),
            ("text/accent2-highlight", t.text, a2.highlight, TEXT_MIN),
            ("accent2-mark/paper", a2.mark, t.background, NON_TEXT_MIN),
            (
                "accent2-mark/sheet",
                a2.mark,
                t.surface_raised,
                NON_TEXT_MIN,
            ),
        ]);
    }
    out
}

/// One line per pair under its minimum, prefixed with the app and theme names.
pub fn findings(app: &str, theme_name: &str, t: &Theme, accent2: Option<&Accent2>) -> Vec<String> {
    pairs(t, accent2)
        .into_iter()
        .filter_map(|(name, fg, bg, min)| {
            let ratio = contrast_ratio(fg, bg);
            (ratio < min).then(|| format!("{app} {theme_name}: {name}: {ratio:.2} < {min}"))
        })
        .collect()
}
