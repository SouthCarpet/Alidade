//! Reads `../../tokens/base.json` + `../../tokens/apps/*.json` (Task 1's
//! JSON, the single source of truth), strictly deserializes them
//! (`deny_unknown_fields` — an invalid file panics the build, which IS the
//! schema gate), expands each app's accent family / neutrals / status /
//! dataviz series against the shared formulas (council canon corrections,
//! plan 051), and writes three generated files to `OUT_DIR`:
//! - `apps_mod.rs`  — one `pub mod <app>` per discovered app JSON file, each
//!   with `LIGHT`/`DARK` typed [`Theme`] consts, `SERIES_HUES`,
//!   `HISTORY_CHROMA_FACTOR` (included by `src/lib.rs`'s `pub mod apps`).
//! - `scale.rs`     — the shared, theme-invariant space/radius/type/motion/
//!   control-h/shadow-raise consts (included by `src/lib.rs`'s `pub mod scale`).
//! - `css_render.rs` — a `stylesheet(app: &str) -> Option<String>` function
//!   whose match arms are pre-rendered, full `:root{...}` CSS text per app,
//!   using `oklch()` strings (included by `src/main.rs`).
//!
//! `src/color.rs` and `src/schema.rs` are pulled in with `include!` rather
//! than as crate dependencies: build.rs is its own compilation, outside the
//! library's module tree, so this is the only way to share the exact same
//! OKLCH math and deny-unknown-fields structs that the library (tests) and
//! this script both need — one implementation, no drift.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

#[allow(dead_code)]
mod color {
    include!("src/color.rs");
}

#[allow(dead_code)]
mod schema {
    include!("src/schema.rs");
}

use color::oklch_to_srgb;
use schema::{parse_app, parse_base, AppTokens, BaseTokens, ColorSpec, Dataviz, ShadowSpec};

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("cargo always sets CARGO_MANIFEST_DIR");
    let out_dir = env::var("OUT_DIR").expect("cargo always sets OUT_DIR");
    let tokens_dir = Path::new(&manifest_dir).join("../../tokens");
    let base_path = tokens_dir.join("base.json");
    let apps_dir = tokens_dir.join("apps");

    println!("cargo:rerun-if-changed={}", base_path.display());
    println!("cargo:rerun-if-changed={}", apps_dir.display());

    let base_text = read_or_panic(&base_path);
    let base: BaseTokens = parse_base(&base_text).unwrap_or_else(|e| {
        panic!(
            "design-tokens schema gate: {} failed schema validation (deny_unknown_fields): {e}",
            base_path.display()
        )
    });

    let mut app_files: Vec<PathBuf> = fs::read_dir(&apps_dir)
        .unwrap_or_else(|e| panic!("design-tokens schema gate: cannot list {}: {e}", apps_dir.display()))
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("json"))
        .collect();
    app_files.sort(); // deterministic module/match-arm order across builds

    let mut apps_mod_src = String::new();
    let mut css_arms = String::new();

    for path in &app_files {
        println!("cargo:rerun-if-changed={}", path.display());

        let kebab = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_else(|| panic!("design-tokens schema gate: unreadable file stem for {}", path.display()))
            .to_string();
        let module_name = kebab.replace('-', "_");

        let text = read_or_panic(path);
        let app: AppTokens = parse_app(&text).unwrap_or_else(|e| {
            panic!(
                "design-tokens schema gate: {} failed schema validation (deny_unknown_fields): {e}",
                path.display()
            )
        });

        let series_hues = derive_series_hues(&kebab, app.accent_hue, &base.dataviz, &app.series_order);
        let light = ThemeValues::build("light", &base, &app, &series_hues);
        let dark = ThemeValues::build("dark", &base, &app, &series_hues);

        let _ = write!(
            apps_mod_src,
            "pub mod {module_name} {{\n    pub static LIGHT: crate::Theme = {light_lit};\n    pub static DARK: crate::Theme = {dark_lit};\n    pub static SERIES_HUES: [f64; 6] = {hues_lit};\n    pub const HISTORY_CHROMA_FACTOR: f64 = {history:?}f64;\n}}\n",
            light_lit = light.rust_theme_literal(),
            dark_lit = dark.rust_theme_literal(),
            hues_lit = format_hues_array(&series_hues),
            history = base.dataviz.history_chroma_factor,
        );

        let stylesheet = render_stylesheet(&light, &dark, &base);
        let _ = write!(css_arms, "        {kebab:?} => Some({stylesheet:?}.to_string()),\n");
    }

    fs::write(Path::new(&out_dir).join("apps_mod.rs"), apps_mod_src).expect("write apps_mod.rs");
    fs::write(Path::new(&out_dir).join("scale.rs"), render_scale(&base)).expect("write scale.rs");
    fs::write(
        Path::new(&out_dir).join("css_render.rs"),
        format!("pub fn stylesheet(app: &str) -> Option<String> {{\n    match app {{\n{css_arms}        _ => None,\n    }}\n}}\n"),
    )
    .expect("write css_render.rs");
}

fn read_or_panic(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("design-tokens schema gate: cannot read {}: {e}", path.display()))
}

/// Series 1 = app accent hue; slots 2-6 = the first `anchor_slots` entries
/// of `anchor_hues` that are NOT within `accent_exclusion_deg` of the
/// accent hue (council canon correction 3), padded with `spare_hue` if
/// fewer survive. If the app supplies an explicit `series_order` of 6 or
/// more hues, that wins outright, truncated to the first 6. A
/// `series_order` SHORTER than 6 is never cycled (controller fix round 1,
/// ruling B3 refined) — see [`fill_short_series_order`]. Either path is
/// asserted to land on exactly 6 entries: that's the fixed `[f64; 6]` the
/// crate's public contract promises.
fn derive_series_hues(kebab: &str, accent_hue: f64, dv: &Dataviz, series_order: &Option<Vec<f64>>) -> [f64; 6] {
    let hues: Vec<f64> = match series_order {
        Some(explicit) if explicit.len() >= 6 => explicit[..6].to_vec(),
        Some(explicit) if !explicit.is_empty() => fill_short_series_order(explicit, accent_hue, dv, kebab),
        _ => {
            assert_eq!(
                1 + dv.anchor_slots,
                6,
                "design-tokens schema gate: dataviz.anchor_slots ({}) + 1 accent slot must equal the fixed 6-series contract",
                dv.anchor_slots
            );
            let mut result = vec![accent_hue];
            result.extend(default_series_tail(accent_hue, dv));
            result
        }
    };
    assert_eq!(
        hues.len(),
        6,
        "design-tokens: {kebab}: derived SERIES_HUES must have exactly 6 entries, got {}",
        hues.len()
    );
    let mut out = [0.0f64; 6];
    out.copy_from_slice(&hues);
    out
}

/// Default (no explicit `series_order`) tail derivation: anchors filtered
/// against the ACCENT hue only (never against each other), padded with
/// `spare_hue` if fewer than `anchor_slots` survive. Boundary is inclusive
/// per controller fix round 1, minor item 4 — an anchor exactly
/// `accent_exclusion_deg` away IS excluded (`<=` excludes, so keeping
/// requires strictly `>`).
fn default_series_tail(accent_hue: f64, dv: &Dataviz) -> Vec<f64> {
    let mut filtered: Vec<f64> = dv
        .anchor_hues
        .iter()
        .copied()
        .filter(|&h| circular_diff(h, accent_hue) > dv.accent_exclusion_deg)
        .collect();
    while filtered.len() < dv.anchor_slots {
        filtered.push(dv.spare_hue);
    }
    filtered.truncate(dv.anchor_slots);
    filtered
}

/// Tail-fill for an explicit `series_order` shorter than 6 (controller fix
/// round 1, ruling B3 refined): anchors (in `anchor_hues` order) are
/// accepted only if they clear `accent_exclusion_deg` (circular, inclusive
/// boundary — same `<=` -excludes rule as [`default_series_tail`]) from
/// the accent hue AND from every hue already spoken for — the explicit
/// list, growing as anchors get accepted, so a later anchor can be
/// rejected by an earlier ACCEPTED one. Never cycles the explicit list.
/// `spare_hue` is tried once per still-missing slot under the same rule;
/// if it also collides, reaching 6 non-colliding series hues is
/// impossible with this app's tokens and the build panics with the exact
/// reason instead of guessing or duplicating.
fn fill_short_series_order(explicit: &[f64], accent_hue: f64, dv: &Dataviz, kebab: &str) -> Vec<f64> {
    let mut used: Vec<f64> = explicit.to_vec();
    used.push(accent_hue); // harmless if already present; only feeds the distance check below
    let needed = 6 - explicit.len();

    let mut result = explicit.to_vec();
    let mut accepted = 0usize;
    for &anchor in &dv.anchor_hues {
        if accepted >= needed {
            break;
        }
        let collides = used.iter().any(|&h| circular_diff(h, anchor) <= dv.accent_exclusion_deg);
        if !collides {
            used.push(anchor);
            result.push(anchor);
            accepted += 1;
        }
    }
    while accepted < needed {
        let collides = used.iter().any(|&h| circular_diff(h, dv.spare_hue) <= dv.accent_exclusion_deg);
        if collides {
            panic!(
                "design-tokens schema gate: {kebab}: series_order has {given} hue(s) but needs 6 — every remaining anchor_hues entry and spare_hue ({spare}) collides (within {deg} degrees) with a hue already in the series {used:?}; add more series_order entries, or widen anchor_hues/spare_hue in base.json",
                given = explicit.len(),
                spare = dv.spare_hue,
                deg = dv.accent_exclusion_deg,
            );
        }
        used.push(dv.spare_hue);
        result.push(dv.spare_hue);
        accepted += 1;
    }
    result
}

fn circular_diff(a: f64, b: f64) -> f64 {
    let d = (a - b).abs() % 360.0;
    if d > 180.0 {
        360.0 - d
    } else {
        d
    }
}

fn format_hues_array(hues: &[f64; 6]) -> String {
    let parts: Vec<String> = hues.iter().map(|h| format!("{h:?}f64")).collect();
    format!("[{}]", parts.join(", "))
}

/// Round to 4 decimals and trim trailing zeros / a trailing '.', so
/// `0.13 * 0.35` (which is `0.045499999999999996` in f64) renders as the
/// clean `0.0455` a hand-authored stylesheet would show, not raw float
/// noise.
fn fmt_num(x: f64) -> String {
    let rounded = (x * 10000.0).round() / 10000.0;
    let s = format!("{rounded:.4}");
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-0" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

/// One OKLCH colour value, still in L/C/H/alpha form. Kept around (instead
/// of converting straight to sRGB) because it feeds two different
/// renderers: the typed `Rgba` const (`rust_lit`, for the library) and the
/// `oklch()` CSS string (`css`, for the stylesheet binary) — both need the
/// same source numbers, at different targets.
#[derive(Clone, Copy)]
struct Cv {
    l: f64,
    c: f64,
    h: f64,
    a: f64,
}

impl Cv {
    fn new(l: f64, c: f64, h: f64, a: f64) -> Self {
        Cv { l, c, h, a }
    }

    fn from_spec(spec: &ColorSpec) -> Self {
        Cv::new(spec.l, spec.c, spec.h, spec.a)
    }

    fn rust_lit(&self) -> String {
        let (r, g, b) = oklch_to_srgb(self.l, self.c, self.h);
        let a = self.a as f32;
        format!("crate::Rgba::new({r:.9}f32, {g:.9}f32, {b:.9}f32, {a:.9}f32)")
    }

    fn css(&self) -> String {
        if (self.a - 1.0).abs() < 1e-9 {
            format!("oklch({} {} {})", fmt_num(self.l), fmt_num(self.c), fmt_num(self.h))
        } else {
            format!(
                "oklch({} {} {} / {})",
                fmt_num(self.l),
                fmt_num(self.c),
                fmt_num(self.h),
                fmt_num(self.a)
            )
        }
    }
}

/// All of [`crate::Theme`]'s colours, still as [`Cv`] (OKLCH), for one app
/// in one mode. Built once per (app, mode) and rendered twice — as a typed
/// Rust const and as a CSS custom-property block.
struct ThemeValues {
    background: Cv,
    well: Cv,
    surface_raised: Cv,
    surface_hover: Cv,
    border: Cv,
    border_top: Cv,
    milled: Cv,
    text: Cv,
    text_muted: Cv,
    disabled_fill: Cv,
    scrim: Cv,
    accent: Cv,
    accent_hover: Cv,
    accent_active: Cv,
    accent_soft: Cv,
    accent_soft_active: Cv,
    accent_soft_foreground: Cv,
    focus: Cv,
    on_accent: Cv,
    danger: Cv,
    danger_hover: Cv,
    danger_active: Cv,
    danger_soft: Cv,
    warning: Cv,
    warning_soft: Cv,
    success: Cv,
    success_soft: Cv,
    chart: [Cv; 6],
    chart_band_alpha: f64,
    chart_grid: Cv,
}

impl ThemeValues {
    fn build(mode: &str, base: &BaseTokens, app: &AppTokens, series_hues: &[f64; 6]) -> Self {
        let neutrals = if mode == "light" { &base.neutrals.light } else { &base.neutrals.dark };
        let af = if mode == "light" { &base.accent_family.light } else { &base.accent_family.dark };
        let status = if mode == "light" { &base.status.light } else { &base.status.dark };
        let series_color = if mode == "light" {
            &base.dataviz.series_color.light
        } else {
            &base.dataviz.series_color.dark
        };

        let tint_h = app.neutral_tint.hue;
        let tint_c = app.neutral_tint.chroma;
        let accent_hue = app.accent_hue;

        // Neutrals are re-hued per app: same L (and the field's own alpha),
        // hue/chroma swapped to the app's neutral_tint. Exceptions: milled
        // and disabled-fill are NEVER retinted (Ruling B2, plan 051 Part B,
        // controller fix round 1) — milled is a white-alpha highlight
        // overlay, disabled-fill is deliberately achromatic (base.json
        // authors both with c:0 / a neutral hue); both are passed through
        // via `Cv::from_spec` at their build.rs call sites below instead of
        // through this closure.
        let neutral = |spec: &ColorSpec| Cv::new(spec.l, tint_c, tint_h, spec.a);

        // Accent family: council canon correction 1 (principle 8),
        // verbatim formulas from base.json's accent_family template.
        let accent_l = af.accent.l;
        let accent_c = af.accent.c;
        let accent = Cv::new(accent_l, accent_c, accent_hue, 1.0);
        let accent_hover = Cv::new(accent_l + af.hover.l_delta, accent_c + af.hover.c_delta, accent_hue, 1.0);
        let accent_active = Cv::new(accent_l + af.active.l_delta, accent_c + af.active.c_delta, accent_hue, 1.0);
        let soft_c = af.soft.c_max.min(accent_c * af.soft.c_factor);
        let accent_soft = Cv::new(af.soft.l, soft_c, accent_hue, 1.0);
        let accent_soft_active = Cv::new(af.soft.l, soft_c + af.soft_active.c_delta, accent_hue, 1.0);
        let softfg_c = accent_c * af.soft_foreground.c_factor;
        let accent_soft_foreground = Cv::new(af.soft_foreground.l, softfg_c, accent_hue, 1.0);
        let focus_c = af.focus.c.unwrap_or(accent_c);
        let focus = Cv::new(af.focus.l, focus_c, accent_hue, 1.0);
        let on_accent = Cv::new(af.on_accent.l, af.on_accent.c, af.on_accent.h, 1.0);

        // Status: council canon correction 2 (principle 9) — literal
        // l/c/h already baked into base.json, theme-invariant across apps
        // (no per-app hue/tint applied).
        let status_color = |spec: &ColorSpec| Cv::from_spec(spec);

        // Dataviz: series colour L/C per theme, hue per series_hues (the
        // slot-rule / explicit-override result computed by the caller).
        let chart: Vec<Cv> = series_hues
            .iter()
            .map(|&h| Cv::new(series_color.l, series_color.c, h, 1.0))
            .collect();
        let chart_arr: [Cv; 6] = [chart[0], chart[1], chart[2], chart[3], chart[4], chart[5]];
        // "chart-grid: border at 40% alpha" — border's app-tinted l/c/h,
        // alpha replaced by dataviz.grid_alpha.
        let chart_grid = Cv::new(neutrals.border.l, tint_c, tint_h, base.dataviz.grid_alpha);

        // Derived neutral, Ruling B5 (plan 051 Part B, T3 review): a single
        // source of truth for S4 residual 1 ("secondary hover = surface
        // shift"), shared by web (CSS var) and iced alike, instead of each
        // consumer re-deriving it (the web gallery previously faked it with
        // color-mix()). L shifts from surface_raised (already app-tinted);
        // same C/H/alpha.
        let surface_raised = neutral(&neutrals.surface_raised);
        let surface_hover_l_delta = if mode == "light" { -0.02 } else { 0.03 };
        let surface_hover = Cv::new(
            surface_raised.l + surface_hover_l_delta,
            surface_raised.c,
            surface_raised.h,
            surface_raised.a,
        );

        ThemeValues {
            background: neutral(&neutrals.background),
            well: neutral(&neutrals.well),
            surface_raised,
            surface_hover,
            border: neutral(&neutrals.border),
            border_top: neutral(&neutrals.border_top),
            milled: Cv::from_spec(&neutrals.milled), // Ruling B2: never retinted
            text: neutral(&neutrals.text),
            text_muted: neutral(&neutrals.text_muted),
            disabled_fill: Cv::from_spec(&neutrals.disabled_fill), // Ruling B2: never retinted
            scrim: neutral(&neutrals.scrim),
            accent,
            accent_hover,
            accent_active,
            accent_soft,
            accent_soft_active,
            accent_soft_foreground,
            focus,
            on_accent,
            danger: status_color(&status.danger),
            danger_hover: status_color(&status.danger_hover),
            danger_active: status_color(&status.danger_active),
            danger_soft: status_color(&status.danger_soft),
            warning: status_color(&status.warning),
            warning_soft: status_color(&status.warning_soft),
            success: status_color(&status.success),
            success_soft: status_color(&status.success_soft),
            chart: chart_arr,
            chart_band_alpha: base.dataviz.band_alpha,
            chart_grid,
        }
    }

    fn rust_theme_literal(&self) -> String {
        format!(
            "crate::Theme {{ background: {background}, well: {well}, surface_raised: {surface_raised}, surface_hover: {surface_hover}, border: {border}, border_top: {border_top}, milled: {milled}, text: {text}, text_muted: {text_muted}, disabled_fill: {disabled_fill}, scrim: {scrim}, accent: {accent}, accent_hover: {accent_hover}, accent_active: {accent_active}, accent_soft: {accent_soft}, accent_soft_active: {accent_soft_active}, accent_soft_foreground: {accent_soft_foreground}, focus: {focus}, on_accent: {on_accent}, danger: {danger}, danger_hover: {danger_hover}, danger_active: {danger_active}, danger_soft: {danger_soft}, warning: {warning}, warning_soft: {warning_soft}, success: {success}, success_soft: {success_soft}, chart: [{c0}, {c1}, {c2}, {c3}, {c4}, {c5}], chart_band_alpha: {band_alpha:?}f32, chart_grid: {chart_grid} }}",
            background = self.background.rust_lit(),
            well = self.well.rust_lit(),
            surface_raised = self.surface_raised.rust_lit(),
            surface_hover = self.surface_hover.rust_lit(),
            border = self.border.rust_lit(),
            border_top = self.border_top.rust_lit(),
            milled = self.milled.rust_lit(),
            text = self.text.rust_lit(),
            text_muted = self.text_muted.rust_lit(),
            disabled_fill = self.disabled_fill.rust_lit(),
            scrim = self.scrim.rust_lit(),
            accent = self.accent.rust_lit(),
            accent_hover = self.accent_hover.rust_lit(),
            accent_active = self.accent_active.rust_lit(),
            accent_soft = self.accent_soft.rust_lit(),
            accent_soft_active = self.accent_soft_active.rust_lit(),
            accent_soft_foreground = self.accent_soft_foreground.rust_lit(),
            focus = self.focus.rust_lit(),
            on_accent = self.on_accent.rust_lit(),
            danger = self.danger.rust_lit(),
            danger_hover = self.danger_hover.rust_lit(),
            danger_active = self.danger_active.rust_lit(),
            danger_soft = self.danger_soft.rust_lit(),
            warning = self.warning.rust_lit(),
            warning_soft = self.warning_soft.rust_lit(),
            success = self.success.rust_lit(),
            success_soft = self.success_soft.rust_lit(),
            c0 = self.chart[0].rust_lit(),
            c1 = self.chart[1].rust_lit(),
            c2 = self.chart[2].rust_lit(),
            c3 = self.chart[3].rust_lit(),
            c4 = self.chart[4].rust_lit(),
            c5 = self.chart[5].rust_lit(),
            band_alpha = self.chart_band_alpha as f32,
            chart_grid = self.chart_grid.rust_lit(),
        )
    }
}

/// Full `:root{...}` + `[data-theme="dark"]{...}` stylesheet text for one
/// app, with `oklch()` strings shaped like the authoritative token table.
fn render_stylesheet(light: &ThemeValues, dark: &ThemeValues, base: &BaseTokens) -> String {
    let mut root = String::new();
    push_color_lines(&mut root, light);
    let t = &base.type_scale;
    let _ = write!(root, "  --font-sans: {};\n", t.font_sans);
    let _ = write!(root, "  --text-xs: {}px;\n", fmt_num(t.xs));
    let _ = write!(root, "  --text-sm: {}px;\n", fmt_num(t.sm));
    let _ = write!(root, "  --text-md: {}px;\n", fmt_num(t.md));
    let _ = write!(root, "  --text-lg: {}px;\n", fmt_num(t.lg));
    let _ = write!(root, "  --text-xl: {}px;\n", fmt_num(t.xl));
    let _ = write!(root, "  --text-2xl: {}px;\n", fmt_num(t.xxl));
    let _ = write!(root, "  --text-3xl: {}px;\n", fmt_num(t.xxxl));
    let sp = &base.space;
    let _ = write!(root, "  --space-1: {}px;\n", fmt_num(sp.s1));
    let _ = write!(root, "  --space-2: {}px;\n", fmt_num(sp.s2));
    let _ = write!(root, "  --space-3: {}px;\n", fmt_num(sp.s3));
    let _ = write!(root, "  --space-4: {}px;\n", fmt_num(sp.s4));
    let _ = write!(root, "  --space-6: {}px;\n", fmt_num(sp.s6));
    let _ = write!(root, "  --space-8: {}px;\n", fmt_num(sp.s8));
    let rad = &base.radius;
    let _ = write!(root, "  --radius-sm: {}px;\n", fmt_num(rad.sm));
    let _ = write!(root, "  --radius-md: {}px;\n", fmt_num(rad.md));
    let _ = write!(root, "  --radius-lg: {}px;\n", fmt_num(rad.lg));
    let _ = write!(root, "  --radius-pill: {}px;\n", fmt_num(rad.pill));
    let _ = write!(root, "  --shadow-raise: {};\n", shadow_css(&base.shadow.raise.light));
    let m = &base.motion;
    let _ = write!(root, "  --motion-fast: {}ms;\n", fmt_num(m.fast));
    let _ = write!(root, "  --motion-base: {}ms;\n", fmt_num(m.base));
    let _ = write!(root, "  --motion-draw: {}ms;\n", fmt_num(m.draw));
    let _ = write!(root, "  --motion-live: {}ms;\n", fmt_num(m.live));
    let _ = write!(root, "  --ease-standard: {};\n", m.ease_standard);
    let _ = write!(root, "  --control-h: {}px;\n", fmt_num(base.misc.control_h));

    let mut dark_block = String::new();
    push_color_lines(&mut dark_block, dark);
    let _ = write!(dark_block, "  --shadow-raise: {};\n", shadow_css(&base.shadow.raise.dark));

    format!(":root {{\n{root}}}\n[data-theme=\"dark\"] {{\n{dark_block}}}\n")
}

fn shadow_css(spec: &ShadowSpec) -> String {
    format!(
        "{} {} {} {}",
        len_px(spec.x),
        len_px(spec.y),
        len_px(spec.blur),
        Cv::from_spec(&spec.color).css()
    )
}

/// A CSS length in px, except a value that rounds to exactly zero renders
/// as bare `0` (no unit) — matches the authoritative table's
/// `--shadow-raise: 0 1px 4px oklch(...)` shape (controller fix round 1,
/// minor item 3).
fn len_px(x: f64) -> String {
    let n = fmt_num(x);
    if n == "0" {
        n
    } else {
        format!("{n}px")
    }
}

fn push_color_lines(out: &mut String, t: &ThemeValues) {
    let _ = write!(out, "  --color-background: {};\n", t.background.css());
    let _ = write!(out, "  --color-well: {};\n", t.well.css());
    let _ = write!(out, "  --color-surface-raised: {};\n", t.surface_raised.css());
    let _ = write!(out, "  --color-surface-hover: {};\n", t.surface_hover.css());
    let _ = write!(out, "  --color-border: {};\n", t.border.css());
    let _ = write!(out, "  --color-border-top: {};\n", t.border_top.css());
    let _ = write!(out, "  --color-milled: {};\n", t.milled.css());
    let _ = write!(out, "  --color-text: {};\n", t.text.css());
    let _ = write!(out, "  --color-text-muted: {};\n", t.text_muted.css());
    let _ = write!(out, "  --color-disabled-fill: {};\n", t.disabled_fill.css());
    let _ = write!(out, "  --color-scrim: {};\n", t.scrim.css());
    let _ = write!(out, "  --color-accent: {};\n", t.accent.css());
    let _ = write!(out, "  --color-accent-hover: {};\n", t.accent_hover.css());
    let _ = write!(out, "  --color-accent-active: {};\n", t.accent_active.css());
    let _ = write!(out, "  --color-accent-soft: {};\n", t.accent_soft.css());
    let _ = write!(out, "  --color-accent-soft-active: {};\n", t.accent_soft_active.css());
    let _ = write!(out, "  --color-accent-soft-foreground: {};\n", t.accent_soft_foreground.css());
    let _ = write!(out, "  --color-focus: {};\n", t.focus.css());
    let _ = write!(out, "  --color-on-accent: {};\n", t.on_accent.css());
    let _ = write!(out, "  --color-danger: {};\n", t.danger.css());
    let _ = write!(out, "  --color-danger-hover: {};\n", t.danger_hover.css());
    let _ = write!(out, "  --color-danger-active: {};\n", t.danger_active.css());
    let _ = write!(out, "  --color-danger-soft: {};\n", t.danger_soft.css());
    let _ = write!(out, "  --color-warning: {};\n", t.warning.css());
    let _ = write!(out, "  --color-warning-soft: {};\n", t.warning_soft.css());
    let _ = write!(out, "  --color-success: {};\n", t.success.css());
    let _ = write!(out, "  --color-success-soft: {};\n", t.success_soft.css());
    for (i, c) in t.chart.iter().enumerate() {
        let _ = write!(out, "  --color-chart-{}: {};\n", i + 1, c.css());
    }
    let _ = write!(out, "  --color-chart-band-alpha: {};\n", fmt_num(t.chart_band_alpha));
    let _ = write!(out, "  --color-chart-grid: {};\n", t.chart_grid.css());
}

/// Theme-invariant scale consts (space/radius/type/motion/control-h) plus
/// the raise shadow, which still varies light/dark by colour so it carries
/// its own light/dark pair rather than living on [`crate::Theme`].
fn render_scale(base: &BaseTokens) -> String {
    let space = &base.space;
    let radius = &base.radius;
    let t = &base.type_scale;
    let m = &base.motion;
    let shadow_light_color = Cv::from_spec(&base.shadow.raise.light.color).rust_lit();
    let shadow_dark_color = Cv::from_spec(&base.shadow.raise.dark.color).rust_lit();

    let mut out = String::new();
    out.push_str("#[derive(Clone, Copy, Debug)]\n");
    out.push_str("pub struct Space { pub s1: f32, pub s2: f32, pub s3: f32, pub s4: f32, pub s6: f32, pub s8: f32 }\n");
    out.push_str("#[derive(Clone, Copy, Debug)]\n");
    out.push_str("pub struct Radius { pub sm: f32, pub md: f32, pub lg: f32, pub pill: f32 }\n");
    out.push_str("#[derive(Clone, Copy, Debug)]\n");
    out.push_str("pub struct TypeScale { pub xs: f32, pub sm: f32, pub md: f32, pub lg: f32, pub xl: f32, pub xxl: f32, pub xxxl: f32, pub font_sans: &'static str, pub numerals: &'static str }\n");
    out.push_str("#[derive(Clone, Copy, Debug)]\n");
    out.push_str("pub struct Motion { pub fast: f32, pub base: f32, pub draw: f32, pub live: f32, pub ease_standard: &'static str }\n");
    out.push_str("#[derive(Clone, Copy, Debug)]\n");
    out.push_str("pub struct ShadowRaise { pub x: f32, pub y: f32, pub blur: f32, pub color: crate::Rgba }\n\n");

    let _ = write!(
        out,
        "pub static SPACE: Space = Space {{ s1: {:?}f32, s2: {:?}f32, s3: {:?}f32, s4: {:?}f32, s6: {:?}f32, s8: {:?}f32 }};\n",
        space.s1 as f32, space.s2 as f32, space.s3 as f32, space.s4 as f32, space.s6 as f32, space.s8 as f32
    );
    let _ = write!(
        out,
        "pub static RADIUS: Radius = Radius {{ sm: {:?}f32, md: {:?}f32, lg: {:?}f32, pill: {:?}f32 }};\n",
        radius.sm as f32, radius.md as f32, radius.lg as f32, radius.pill as f32
    );
    let _ = write!(
        out,
        "pub static TYPE: TypeScale = TypeScale {{ xs: {:?}f32, sm: {:?}f32, md: {:?}f32, lg: {:?}f32, xl: {:?}f32, xxl: {:?}f32, xxxl: {:?}f32, font_sans: {:?}, numerals: {:?} }};\n",
        t.xs as f32, t.sm as f32, t.md as f32, t.lg as f32, t.xl as f32, t.xxl as f32, t.xxxl as f32, t.font_sans, t.numerals
    );
    let _ = write!(
        out,
        "pub static MOTION: Motion = Motion {{ fast: {:?}f32, base: {:?}f32, draw: {:?}f32, live: {:?}f32, ease_standard: {:?} }};\n",
        m.fast as f32, m.base as f32, m.draw as f32, m.live as f32, m.ease_standard
    );
    let _ = write!(out, "pub const CONTROL_H: f32 = {:?}f32;\n", base.misc.control_h as f32);
    let _ = write!(
        out,
        "pub static SHADOW_RAISE_LIGHT: ShadowRaise = ShadowRaise {{ x: {:?}f32, y: {:?}f32, blur: {:?}f32, color: {} }};\n",
        base.shadow.raise.light.x as f32, base.shadow.raise.light.y as f32, base.shadow.raise.light.blur as f32, shadow_light_color
    );
    let _ = write!(
        out,
        "pub static SHADOW_RAISE_DARK: ShadowRaise = ShadowRaise {{ x: {:?}f32, y: {:?}f32, blur: {:?}f32, color: {} }};\n",
        base.shadow.raise.dark.x as f32, base.shadow.raise.dark.y as f32, base.shadow.raise.dark.blur as f32, shadow_dark_color
    );

    out
}
