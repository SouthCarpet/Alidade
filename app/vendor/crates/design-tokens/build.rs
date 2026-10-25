//! Reads `../../tokens/base.json` + `../../tokens/apps/*.json` (Task 1's
//! JSON, the single source of truth), strictly deserializes them
//! (`deny_unknown_fields` — an invalid file panics the build, which IS the
//! schema gate), expands each app's accent family / neutrals / status /
//! dataviz series against the shared formulas (council canon corrections,
//! plan 051), and writes three generated files to `OUT_DIR`:
//! - `apps_mod.rs`  — one `pub mod <app>` per discovered app JSON file, each
//!   with `LIGHT`/`DARK` typed [`Theme`] consts and
//!   `HISTORY_CHROMA_FACTOR` (included by `src/lib.rs`'s `pub mod apps`).
//! - `scale.rs`     — the shared, theme-invariant space/radius/type/motion/
//!   control-h consts (included by `src/lib.rs`'s `pub mod scale`).
//! - `css_render.rs`: a `stylesheet_pairs(app: &str) -> Option<CssPairs>`
//!   function whose match arms are pre-rendered `(name, value)` pairs per
//!   app (item C), using `oklch()` value strings (included by
//!   `src/main.rs`). Assembled into final CSS text at CLI runtime by
//!   `css_assemble::assemble_stylesheet`, once the `--prefix`/
//!   `--dark-selector` flags are known. Baking a final string here, as
//!   before item C, would leave no way to prefix a name without a
//!   string-replace over already-rendered text.
//!
//! It also runs the hue-collision gate (item D) over every discovered app
//! and writes a fourth file, `hues_table.txt` (included by `src/main.rs`'s
//! `hues` subcommand): a build failure IS the gate, same as the schema gate.
//!
//! `src/color.rs`, `src/schema.rs` and `src/hue_gate.rs` are pulled in with
//! `include!` rather than as crate dependencies: build.rs is its own
//! compilation, outside the library's module tree, so this is the only way
//! to share the exact same OKLCH math, deny-unknown-fields structs and gate
//! logic that the library (tests) and this script both need, one
//! implementation, no drift.

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

#[allow(dead_code)]
mod hue_gate {
    include!("src/hue_gate.rs");
}

use color::oklch_to_srgb;
use hue_gate::AppHue;
use schema::{
    parse_app, parse_base, AppTokens, BaseTokens, ColorPatch, ColorSpec, ExtraOverrideField,
    Overrides, ShadowLayerOverride, ThemeOverride,
};

/// One CSS custom-property `(name, value)` pair, name without the leading
/// `--`. `render_css_pairs`/`color_pairs`/`format_pairs_array` build and
/// render these; `css_assemble::assemble_stylesheet` (a separate module,
/// called at CLI runtime) turns them into final text.
type CssPairs = Vec<(String, String)>;

/// Item D, narrowed by thesis check F4: app-to-app hue distance is reported
/// unconditionally by `design-tokens hues`, but only ENFORCED below this
/// threshold (well inside `dataviz.accent_exclusion_deg`, which gates
/// distance to the status hues instead). Not a token: this is a gate
/// parameter, not a rendered value, so it has no base.json field.
const APP_TO_APP_DEG: f64 = 8.0;

/// Reserved extra names (item F): a caller cannot shadow a role the
/// generator already owns.
const RESERVED_EXTRA_NAMES: [&str; 5] = ["accent", "danger", "warning", "success", "chart"];

/// One resolved `formula: "accent"` extra for one theme (plan 094 Task 9,
/// replaces plan 092 phase p6's `second_accent`): base, hover, active, soft
/// and soft-foreground reuse the shared accent_family formula verbatim at
/// the extra's own hue. (The muted `rail` variant left in plan 094
/// revision 2: an identity rail is an AI-slop block device.)
struct ExtraAccentValues {
    name: String,
    accent: Cv,
    hover: Cv,
    active: Cv,
    soft: Cv,
    soft_foreground: Cv,
}

/// One resolved `formula: "series"` extra for one theme (plan 094 Task 9,
/// replaces plan 092 phase p6's `team_rings`): a flat colour from the
/// shared dataviz series_color l/c formula at the extra's own hue.
struct ExtraSeriesValues {
    name: String,
    value: Cv,
}

/// The ink block for one theme (plan 125, spec R25): every part at the accent hue.
struct InkValues {
    fill: Cv,
    text: Cv,
    muted: Cv,
    rule: Cv,
    action: Cv,
    on_action: Cv,
}

impl InkValues {
    fn build(ink: &schema::InkFormula, hue: f64) -> Self {
        let at = |lc: &schema::AccentLc| Cv::new(lc.l, lc.c, hue, 1.0);
        InkValues {
            fill: at(&ink.fill),
            text: at(&ink.text),
            muted: at(&ink.muted),
            rule: at(&ink.rule),
            action: at(&ink.action),
            on_action: at(&ink.on_action),
        }
    }
}

/// The tone tokens of an optional second hue (plan 125, spec R35, lane k12):
/// field at the soft lightness, highlight at the selected lightness, mark at
/// the dataviz.series_color lightness, each quieter than the primary role.
struct Accent2Values {
    field: Cv,
    highlight: Cv,
    mark: Cv,
}

/// One second-hue tone (lane k12): the role's lightness, a chroma tied to the
/// primary chroma of the same role and capped, then lowered at fixed L and H
/// until no sRGB channel clips. Web and iced both use this one result.
fn accent2_tone(l: f64, primary_c: f64, rule: &schema::Accent2Chroma, h: f64) -> Cv {
    let c = color::accent2_chroma(l, primary_c, rule.c_factor, rule.c_max, h);
    Cv::new(l, c, h, 1.0)
}

/// kebab-case: lowercase ASCII letters, digits and hyphens only, first and
/// last character alphanumeric (no leading/trailing hyphen).
fn is_kebab_case(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let bytes = name.as_bytes();
    let valid_char = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-';
    bytes.iter().all(|&b| valid_char(b)) && bytes[0] != b'-' && bytes[bytes.len() - 1] != b'-'
}

fn main() {
    let manifest_dir =
        env::var("CARGO_MANIFEST_DIR").expect("cargo always sets CARGO_MANIFEST_DIR");
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
        .unwrap_or_else(|e| {
            panic!(
                "design-tokens schema gate: cannot list {}: {e}",
                apps_dir.display()
            )
        })
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("json"))
        .collect();
    app_files.sort(); // deterministic module/match-arm order across builds

    let mut apps_mod_src = String::new();
    let mut css_arms = String::new();
    let mut app_registry: Vec<(String, String)> = Vec::new(); // (kebab, module_name), build order
    let mut hue_entries: Vec<AppHue> = Vec::new();

    for path in &app_files {
        println!("cargo:rerun-if-changed={}", path.display());

        let kebab = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_else(|| {
                panic!(
                    "design-tokens schema gate: unreadable file stem for {}",
                    path.display()
                )
            })
            .to_string();
        let module_name = kebab.replace('-', "_");
        app_registry.push((kebab.clone(), module_name.clone()));

        let text = read_or_panic(path);
        let app: AppTokens = parse_app(&text).unwrap_or_else(|e| {
            panic!(
                "design-tokens schema gate: {} failed schema validation (deny_unknown_fields): {e}",
                path.display()
            )
        });

        for extra in &app.extras {
            if !is_kebab_case(&extra.name) {
                panic!(
                    "design-tokens schema gate: {kebab}: extras name {name:?} is not kebab-case (lowercase letters, digits, hyphens; no leading/trailing hyphen)",
                    name = extra.name
                );
            }
            if RESERVED_EXTRA_NAMES.contains(&extra.name.as_str()) {
                panic!(
                    "design-tokens schema gate: {kebab}: extras name {name:?} is a reserved stem ({reserved:?})",
                    name = extra.name,
                    reserved = RESERVED_EXTRA_NAMES
                );
            }
            if extra.formula != "accent" && extra.formula != "series" {
                panic!(
                    "design-tokens schema gate: {kebab}: extras[{name:?}].formula {formula:?} must be \"accent\" or \"series\"",
                    name = extra.name,
                    formula = extra.formula
                );
            }
        }

        hue_entries.push(AppHue {
            name: kebab.clone(),
            accent_hue: app.accent_hue,
            accepted: app
                .accepted_hue_overlaps
                .iter()
                .map(|e| (e.with.clone(), e.reason.clone()))
                .collect(),
        });

        let light = ThemeValues::build("light", &base, &app, app.overrides.as_ref());
        let dark = ThemeValues::build("dark", &base, &app, app.overrides.as_ref());

        let _ = writeln!(
            apps_mod_src,
            "pub mod {module_name} {{\n    pub static LIGHT: crate::Theme = {light_lit};\n    pub static DARK: crate::Theme = {dark_lit};\n    pub static ACCENT2_LIGHT: Option<crate::Accent2> = {light_a2};\n    pub static ACCENT2_DARK: Option<crate::Accent2> = {dark_a2};\n    pub const HISTORY_CHROMA_FACTOR: f64 = {history:?}f64;\n{extras_mod}}}",
            light_lit = light.rust_theme_literal(),
            dark_lit = dark.rust_theme_literal(),
            light_a2 = light.rust_accent2_literal(),
            dark_a2 = dark.rust_accent2_literal(),
            history = base.dataviz.history_chroma_factor,
            extras_mod = render_extras_module(&light, &dark),
        );

        let (light_pairs, dark_pairs) =
            render_css_pairs(&light, &dark, &base, app.overrides.as_ref());
        let _ = writeln!(
            css_arms,
            "        {kebab:?} => Some(({light_arr}, {dark_arr})),",
            light_arr = format_pairs_array(&light_pairs),
            dark_arr = format_pairs_array(&dark_pairs),
        );
    }

    // Item B: a discovery registry every Rust gate can iterate instead of a
    // hardcoded app list (`design_tokens::APPS`, re-exported from `lib.rs`).
    // Lives inside `pub mod apps` (this same generated file) because the
    // entries reference sibling app modules unqualified.
    let _ = writeln!(
        apps_mod_src,
        "pub static APPS: &[(&str, &crate::Theme, &crate::Theme)] = &["
    );
    for (kebab, module_name) in &app_registry {
        let _ = writeln!(
            apps_mod_src,
            "    ({kebab:?}, &{module_name}::LIGHT, &{module_name}::DARK),"
        );
    }
    apps_mod_src.push_str("];\n");
    // Plan 125 (spec R35): the optional second-hue tones, in the same order as APPS.
    apps_mod_src.push_str(
        "pub static ACCENT2: &[(&str, &Option<crate::Accent2>, &Option<crate::Accent2>)] = &[\n",
    );
    for (kebab, module_name) in &app_registry {
        let _ = writeln!(
            apps_mod_src,
            "    ({kebab:?}, &{module_name}::ACCENT2_LIGHT, &{module_name}::ACCENT2_DARK),"
        );
    }
    apps_mod_src.push_str("];\n");

    // Item D: the hue-collision gate. A collision panics the build, exactly
    // like the schema gate above; never silenced by lowering a threshold or
    // repainting a palette, only by an accepted_hue_overlaps entry with a
    // reason (D10 -- Michal decides every hue change).
    let statuses: [(&str, f64); 3] = [
        ("danger", base.status.light.danger.h),
        ("warning", base.status.light.warning.h),
        ("success", base.status.light.success.h),
    ];
    if let Err(e) = hue_gate::check_hue_collisions(
        &hue_entries,
        &statuses,
        base.dataviz.accent_exclusion_deg,
        APP_TO_APP_DEG,
    ) {
        panic!("design-tokens schema gate: {e}");
    }
    let hues_table = hue_gate::render_hues_table(
        &hue_entries,
        &statuses,
        base.dataviz.accent_exclusion_deg,
        APP_TO_APP_DEG,
    );

    fs::write(Path::new(&out_dir).join("apps_mod.rs"), apps_mod_src).expect("write apps_mod.rs");
    fs::write(Path::new(&out_dir).join("scale.rs"), render_scale(&base)).expect("write scale.rs");
    fs::write(Path::new(&out_dir).join("hues_table.txt"), &hues_table)
        .expect("write hues_table.txt");
    fs::write(
        Path::new(&out_dir).join("css_render.rs"),
        format!(
            "type CssPairs = (&'static [(&'static str, &'static str)], &'static [(&'static str, &'static str)]);\npub fn stylesheet_pairs(app: &str) -> Option<CssPairs> {{\n    match app {{\n{css_arms}        _ => None,\n    }}\n}}\n"
        ),
    )
    .expect("write css_render.rs");
}

fn read_or_panic(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| {
        panic!(
            "design-tokens schema gate: cannot read {}: {e}",
            path.display()
        )
    })
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
        format!("crate::Rgba::new({r:?}f32, {g:?}f32, {b:?}f32, {a:?}f32)")
    }

    fn css(&self) -> String {
        if (self.a - 1.0).abs() < 1e-9 {
            format!(
                "oklch({} {} {})",
                fmt_num(self.l),
                fmt_num(self.c),
                fmt_num(self.h)
            )
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

/// Apply a partial l/c/h patch (plan 098, item C1) onto an already-computed
/// [`Cv`]: a component present in `patch` replaces `base`'s, an absent one
/// keeps it. Alpha is never patched by this path (no shipped override needs
/// it).
fn apply_patch(base: Cv, patch: &ColorPatch) -> Cv {
    Cv::new(
        patch.l.unwrap_or(base.l),
        patch.c.unwrap_or(base.c),
        patch.h.unwrap_or(base.h),
        base.a,
    )
}

/// This extra's per-theme override, if the app declared one (plan 098, item
/// C1): looked up by the extra's own `name`, open-ended like `extras`
/// itself.
fn extra_override_for<'a>(
    overrides: Option<&'a Overrides>,
    name: &str,
    mode: &str,
) -> Option<&'a ExtraOverrideField> {
    let entry = overrides?.extras.get(name)?;
    if mode == "light" {
        entry.light.as_ref()
    } else {
        entry.dark.as_ref()
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
    surface_active: Cv,
    border: Cv,
    border_strong: Cv,
    control: Cv,
    text: Cv,
    text_muted: Cv,
    disabled_text: Cv,
    disabled_fill: Cv,
    scrim: Cv,
    ink: InkValues,
    accent2: Option<Accent2Values>,
    accent: Cv,
    accent_hover: Cv,
    accent_active: Cv,
    accent_soft: Cv,
    accent_soft_active: Cv,
    accent_soft_foreground: Cv,
    surface_selected: Cv,
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
    extras_accent: Vec<ExtraAccentValues>,
    extras_series: Vec<ExtraSeriesValues>,
}

/// The light or the dark half of a themed pair of values.
fn by_mode<'a, T>(mode: &str, light: &'a T, dark: &'a T) -> &'a T {
    if mode == "light" {
        light
    } else {
        dark
    }
}

/// `base` with a partial l/c/h patch laid over it, when the app declared one.
fn patched(base: Cv, patch: Option<&ColorPatch>) -> Cv {
    patch.map_or(base, |p| apply_patch(base, p))
}

/// The primary accent's l/c/h input (plan 098, item C1): base.json's accent
/// at the app's hue, with the app's own accent patch laid over each part it
/// names. The deltas and factors stay base.json's.
fn primary_accent_input(
    af: &schema::AccentFamilyTheme,
    hue: f64,
    patch: Option<&ColorPatch>,
) -> (f64, f64, f64) {
    let Some(patch) = patch else {
        return (af.accent.l, af.accent.c, hue);
    };
    (
        patch.l.unwrap_or(af.accent.l),
        patch.c.unwrap_or(af.accent.c),
        patch.h.unwrap_or(hue),
    )
}

/// Extras (plan 094 Task 9, item F): `formula: "accent"` reuses the primary
/// accent formula at the extra's own hue; `formula: "series"` reuses the
/// shared dataviz series_color l/c formula. Names and formulas are already
/// validated in `main`'s discovery loop before this runs. The accent inputs
/// stay base.json's `accent_l`/`accent_c`: a primary-accent override never
/// touches an extra (plan 098, item C1).
fn build_extras(
    mode: &str,
    af: &schema::AccentFamilyTheme,
    series_color: &schema::SeriesColor,
    app: &AppTokens,
    overrides: Option<&Overrides>,
) -> (Vec<ExtraAccentValues>, Vec<ExtraSeriesValues>) {
    let (accent_l, accent_c) = (af.accent.l, af.accent.c);
    let soft_c = af.soft.c_max.min(accent_c * af.soft.c_factor);
    let softfg_c = accent_c * af.soft_foreground.c_factor;
    let mut extras_accent: Vec<ExtraAccentValues> = Vec::new();
    let mut extras_series: Vec<ExtraSeriesValues> = Vec::new();
    for extra in &app.extras {
        let h = extra.hue;
        if extra.formula != "accent" {
            extras_series.push(ExtraSeriesValues {
                name: extra.name.clone(),
                value: Cv::new(series_color.l, series_color.c, h, 1.0),
            });
            continue;
        }
        let soft_patch =
            extra_override_for(overrides, &extra.name, mode).and_then(|e| e.soft.as_ref());
        extras_accent.push(ExtraAccentValues {
            name: extra.name.clone(),
            accent: Cv::new(accent_l, accent_c, h, 1.0),
            hover: Cv::new(
                accent_l + af.hover.l_delta,
                accent_c + af.hover.c_delta,
                h,
                1.0,
            ),
            active: Cv::new(
                accent_l + af.active.l_delta,
                accent_c + af.active.c_delta,
                h,
                1.0,
            ),
            soft: patched(Cv::new(af.soft.l, soft_c, h, 1.0), soft_patch),
            soft_foreground: Cv::new(af.soft_foreground.l, softfg_c, h, 1.0),
        });
    }
    (extras_accent, extras_series)
}

impl ThemeValues {
    fn build(
        mode: &str,
        base: &BaseTokens,
        app: &AppTokens,
        overrides: Option<&Overrides>,
    ) -> Self {
        let neutrals = by_mode(mode, &base.neutrals.light, &base.neutrals.dark);
        let af = by_mode(mode, &base.accent_family.light, &base.accent_family.dark);
        let status = by_mode(mode, &base.status.light, &base.status.dark);
        let series_color = by_mode(
            mode,
            &base.dataviz.series_color.light,
            &base.dataviz.series_color.dark,
        );
        let ramp = by_mode(mode, &base.dataviz.ramp.light, &base.dataviz.ramp.dark);
        assert_eq!(
            ramp.len(),
            6,
            "design-tokens schema gate: dataviz.ramp.{mode} must have exactly 6 steps, got {}",
            ramp.len()
        );
        let theme_override: Option<&ThemeOverride> =
            overrides.and_then(|o| by_mode(mode, &o.light, &o.dark).as_ref());

        let accent_hue = app.accent_hue;

        // Neutrals: an app with a `neutral_tint` re-hues every tinted neutral
        // (same L and the field's own alpha, hue/chroma from the tint). An app
        // without one takes base.json as authored (direction A: hue 200, chroma
        // per role). disabled-fill is NEVER retinted (Ruling B2, plan 051 Part
        // B): it is deliberately achromatic, passed through via
        // `Cv::from_spec` at its call site below instead of this closure.
        let neutral = |spec: &ColorSpec| match &app.neutral_tint {
            Some(tint) => Cv::new(spec.l, tint.chroma, tint.hue, spec.a),
            None => Cv::from_spec(spec),
        };

        // Accent family: council canon correction 1 (principle 8),
        // verbatim formulas from base.json's accent_family template.
        // Primary accent-family override (plan 098, item C1): only this
        // app's OWN accent/hover/active/soft/soft_active/soft_foreground/
        // focus use the override input; the deltas/factors themselves stay
        // base.json's, so they cascade onto the overridden input unchanged.
        let (pri_l, pri_c, pri_h) = primary_accent_input(
            af,
            accent_hue,
            theme_override.and_then(|t| t.accent.as_ref()),
        );
        let pri_soft_c = af.soft.c_max.min(pri_c * af.soft.c_factor);
        let pri_softfg_c = pri_c * af.soft_foreground.c_factor;

        let accent = Cv::new(pri_l, pri_c, pri_h, 1.0);
        let accent_hover = Cv::new(
            pri_l + af.hover.l_delta,
            pri_c + af.hover.c_delta,
            pri_h,
            1.0,
        );
        let accent_active = Cv::new(
            pri_l + af.active.l_delta,
            pri_c + af.active.c_delta,
            pri_h,
            1.0,
        );
        let accent_soft = Cv::new(af.soft.l, pri_soft_c, pri_h, 1.0);
        let accent_soft_active = Cv::new(
            af.soft.l + af.soft_active.l_delta,
            pri_soft_c + af.soft_active.c_delta,
            pri_h,
            1.0,
        );
        let accent_soft_foreground = Cv::new(af.soft_foreground.l, pri_softfg_c, pri_h, 1.0);
        let surface_selected = Cv::new(af.selected.l, pri_c * af.selected.c_factor, pri_h, 1.0);
        let focus_c = af.focus.c.unwrap_or(pri_c);
        let focus_l = theme_override.and_then(|t| t.focus_l).unwrap_or(af.focus.l);
        let focus = Cv::new(focus_l, focus_c, pri_h, 1.0);
        let on_accent = patched(
            Cv::new(af.on_accent.l, af.on_accent.c, af.on_accent.h, 1.0),
            theme_override.and_then(|t| t.on_accent.as_ref()),
        );

        // Status: council canon correction 2 (principle 9) — literal
        // l/c/h already baked into base.json, theme-invariant across apps
        // (no per-app hue/tint applied).
        let status_color = |spec: &ColorSpec| Cv::from_spec(spec);

        // Dataviz (direction A): one-hue ramp. Every series uses the app's
        // own accent hue at the ramp's six lightness/chroma steps; identity
        // comes from the dash pattern, not from the hue.
        let chart_vec: Vec<Cv> = ramp
            .iter()
            .map(|s| Cv::new(s.l, s.c, accent_hue, 1.0))
            .collect();
        let chart_arr: [Cv; 6] = [
            chart_vec[0],
            chart_vec[1],
            chart_vec[2],
            chart_vec[3],
            chart_vec[4],
            chart_vec[5],
        ];
        // chart-grid: the border's own l/c/h (tint applied), alpha replaced
        // by dataviz.grid_alpha for this theme.
        let grid_source = neutral(&neutrals.border);
        let grid_alpha = *by_mode(
            mode,
            &base.dataviz.grid_alpha.light,
            &base.dataviz.grid_alpha.dark,
        );
        let chart_grid = Cv::new(grid_source.l, grid_source.c, grid_source.h, grid_alpha);

        let background = patched(
            neutral(&neutrals.background),
            theme_override.and_then(|t| t.background.as_ref()),
        );

        // surface-hover is authored in base.json (direction A), a real role
        // shared by web (`--color-surface-hover`) and iced. An app's
        // `surface_raised` patch does not move it: patch `surface_hover`
        // itself when the app needs a different hover.
        let surface_raised = patched(
            neutral(&neutrals.surface_raised),
            theme_override.and_then(|t| t.surface_raised.as_ref()),
        );
        let surface_hover = patched(
            neutral(&neutrals.surface_hover),
            theme_override.and_then(|t| t.surface_hover.as_ref()),
        );

        let border = patched(
            neutral(&neutrals.border),
            theme_override.and_then(|t| t.border.as_ref()),
        );

        let (extras_accent, extras_series) = build_extras(mode, af, series_color, app, overrides);

        ThemeValues {
            background,
            well: neutral(&neutrals.well),
            surface_raised,
            surface_hover,
            surface_active: neutral(&neutrals.surface_active),
            border,
            border_strong: neutral(&neutrals.border_strong),
            control: neutral(&neutrals.control),
            text: neutral(&neutrals.text),
            text_muted: neutral(&neutrals.text_muted),
            disabled_text: Cv::from_spec(&neutrals.disabled_text), // achromatic, like disabled-fill
            disabled_fill: Cv::from_spec(&neutrals.disabled_fill), // Ruling B2: never retinted
            scrim: neutral(&neutrals.scrim),
            ink: InkValues::build(&af.ink, pri_h),
            accent2: app.accent2_hue.map(|h| Accent2Values {
                field: accent2_tone(af.soft.l, accent_soft.c, &af.accent2.field, h),
                highlight: accent2_tone(
                    af.selected.l,
                    surface_selected.c,
                    &af.accent2.highlight,
                    h,
                ),
                mark: accent2_tone(series_color.l, accent.c, &af.accent2.mark, h),
            }),
            accent,
            accent_hover,
            accent_active,
            accent_soft,
            accent_soft_active,
            accent_soft_foreground,
            surface_selected,
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
            extras_accent,
            extras_series,
        }
    }

    fn rust_theme_literal(&self) -> String {
        format!(
            "crate::Theme {{ background: {background}, well: {well}, surface_raised: {surface_raised}, surface_hover: {surface_hover}, surface_active: {surface_active}, border: {border}, border_strong: {border_strong}, control: {control}, text: {text}, text_muted: {text_muted}, disabled_text: {disabled_text}, disabled_fill: {disabled_fill}, scrim: {scrim}, ink: {ink}, on_ink: {on_ink}, on_ink_muted: {on_ink_muted}, ink_rule: {ink_rule}, ink_action: {ink_action}, on_ink_action: {on_ink_action}, accent: {accent}, accent_hover: {accent_hover}, accent_active: {accent_active}, accent_soft: {accent_soft}, accent_soft_active: {accent_soft_active}, accent_soft_foreground: {accent_soft_foreground}, surface_selected: {surface_selected}, focus: {focus}, on_accent: {on_accent}, danger: {danger}, danger_hover: {danger_hover}, danger_active: {danger_active}, danger_soft: {danger_soft}, warning: {warning}, warning_soft: {warning_soft}, success: {success}, success_soft: {success_soft}, chart: [{c0}, {c1}, {c2}, {c3}, {c4}, {c5}], chart_band_alpha: {band_alpha:?}f32, chart_grid: {chart_grid} }}",
            background = self.background.rust_lit(),
            well = self.well.rust_lit(),
            surface_raised = self.surface_raised.rust_lit(),
            surface_hover = self.surface_hover.rust_lit(),
            surface_active = self.surface_active.rust_lit(),
            border = self.border.rust_lit(),
            border_strong = self.border_strong.rust_lit(),
            control = self.control.rust_lit(),
            text = self.text.rust_lit(),
            text_muted = self.text_muted.rust_lit(),
            disabled_text = self.disabled_text.rust_lit(),
            disabled_fill = self.disabled_fill.rust_lit(),
            scrim = self.scrim.rust_lit(),
            ink = self.ink.fill.rust_lit(),
            on_ink = self.ink.text.rust_lit(),
            on_ink_muted = self.ink.muted.rust_lit(),
            ink_rule = self.ink.rule.rust_lit(),
            ink_action = self.ink.action.rust_lit(),
            on_ink_action = self.ink.on_action.rust_lit(),
            accent = self.accent.rust_lit(),
            accent_hover = self.accent_hover.rust_lit(),
            accent_active = self.accent_active.rust_lit(),
            accent_soft = self.accent_soft.rust_lit(),
            accent_soft_active = self.accent_soft_active.rust_lit(),
            accent_soft_foreground = self.accent_soft_foreground.rust_lit(),
            surface_selected = self.surface_selected.rust_lit(),
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

    /// `Some(crate::Accent2 { .. })` for an app with `accent2_hue`, else `None`.
    fn rust_accent2_literal(&self) -> String {
        match &self.accent2 {
            Some(a2) => format!(
                "Some(crate::Accent2 {{ field: {}, highlight: {}, mark: {} }})",
                a2.field.rust_lit(),
                a2.highlight.rust_lit(),
                a2.mark.rust_lit()
            ),
            None => "None".to_string(),
        }
    }
}

/// `CONST_NAME` from a kebab-case extra name: uppercase, hyphens to
/// underscores (`team-blue` -> `TEAM_BLUE`).
fn extra_const_stem(name: &str) -> String {
    name.to_uppercase().replace('-', "_")
}

/// `pub mod extras { ... }` for one app (plan 094 Task 9): one
/// `crate::AccentFamily` LIGHT/DARK pair per `formula: "accent"` entry, one
/// `crate::Rgba` LIGHT/DARK pair per `formula: "series"` entry. Empty for
/// an app with no `extras` -- still valid Rust, nothing to import.
fn render_extras_module(light: &ThemeValues, dark: &ThemeValues) -> String {
    let mut out = String::from("    pub mod extras {\n");
    for (light_ea, dark_ea) in light.extras_accent.iter().zip(dark.extras_accent.iter()) {
        let stem = extra_const_stem(&light_ea.name);
        let _ = writeln!(
            out,
            "        pub static {stem}_LIGHT: crate::AccentFamily = crate::AccentFamily {{ accent: {a}, hover: {h}, active: {ac}, soft: {s}, soft_foreground: {sf} }};",
            a = light_ea.accent.rust_lit(),
            h = light_ea.hover.rust_lit(),
            ac = light_ea.active.rust_lit(),
            s = light_ea.soft.rust_lit(),
            sf = light_ea.soft_foreground.rust_lit(),
        );
        let _ = writeln!(
            out,
            "        pub static {stem}_DARK: crate::AccentFamily = crate::AccentFamily {{ accent: {a}, hover: {h}, active: {ac}, soft: {s}, soft_foreground: {sf} }};",
            a = dark_ea.accent.rust_lit(),
            h = dark_ea.hover.rust_lit(),
            ac = dark_ea.active.rust_lit(),
            s = dark_ea.soft.rust_lit(),
            sf = dark_ea.soft_foreground.rust_lit(),
        );
    }
    for (light_es, dark_es) in light.extras_series.iter().zip(dark.extras_series.iter()) {
        let stem = extra_const_stem(&light_es.name);
        let _ = writeln!(
            out,
            "        pub static {stem}_LIGHT: crate::Rgba = {};",
            light_es.value.rust_lit()
        );
        let _ = writeln!(
            out,
            "        pub static {stem}_DARK: crate::Rgba = {};",
            dark_es.value.rust_lit()
        );
    }
    out.push_str("    }\n");
    out
}

/// Every `(name, value)` pair for one app in both modes, in the schema's
/// field order. Light also carries the theme-invariant scale (type,
/// space, radius, motion, control height): declared once, inherited by
/// any `[data-theme]` block since custom properties cascade. The actual
/// `:root{...}`/dark-selector text (with the `--prefix` and
/// `color-scheme` lines) is assembled at CLI runtime by
/// `css_assemble::assemble_stylesheet`, from data, never by a
/// string-replace over rendered CSS text (item C).
fn render_css_pairs(
    light: &ThemeValues,
    dark: &ThemeValues,
    base: &BaseTokens,
    overrides: Option<&Overrides>,
) -> (CssPairs, CssPairs) {
    let new_tokens = overrides.and_then(|o| o.new_tokens.as_ref());
    let accent_text = new_tokens.and_then(|nt| nt.accent_text.as_ref());
    let shadow_override = overrides.and_then(|o| o.shadow_raise.as_ref());
    let motion_override = overrides.and_then(|o| o.motion.as_ref());
    let radius_override = overrides.and_then(|o| o.radius.as_ref());

    // An app that supplies its own `surface_selected` (a literal CSS string,
    // theme-invariant) replaces the base accent-derived fill in BOTH blocks:
    // the base entry is skipped, so the dark block inherits the override
    // through the cascade instead of redeclaring the accent-derived value.
    let own_surface_selected = new_tokens.is_some_and(|nt| nt.surface_selected.is_some());
    let mut light_pairs = color_pairs(light, accent_text.map(|at| &at.light), own_surface_selected);
    // The six chart dash patterns (theme-invariant, so the dark block inherits
    // them): `--chart-dash-N` is a `stroke-dasharray` value, `none` for solid.
    for (i, pattern) in base.dataviz.stroke_patterns.iter().enumerate() {
        light_pairs.push((format!("chart-dash-{}", i + 1), dash_css(&pattern.dash)));
    }
    let t = &base.type_scale;
    light_pairs.push(("font-sans".to_string(), t.font_sans.clone()));
    light_pairs.push(("text-xs".to_string(), format!("{}px", fmt_num(t.xs))));
    light_pairs.push(("text-sm".to_string(), format!("{}px", fmt_num(t.sm))));
    light_pairs.push(("text-body".to_string(), format!("{}px", fmt_num(t.body))));
    light_pairs.push(("text-md".to_string(), format!("{}px", fmt_num(t.md))));
    light_pairs.push(("text-lg".to_string(), format!("{}px", fmt_num(t.lg))));
    light_pairs.push(("text-xl".to_string(), format!("{}px", fmt_num(t.xl))));
    light_pairs.push(("text-2xl".to_string(), format!("{}px", fmt_num(t.xxl))));
    light_pairs.push(("text-3xl".to_string(), format!("{}px", fmt_num(t.xxxl))));
    light_pairs.push(("text-read".to_string(), format!("{}px", fmt_num(t.read))));
    light_pairs.push(("leading-read".to_string(), fmt_num(t.read_leading)));
    light_pairs.push((
        "measure-read".to_string(),
        format!("{}ch", fmt_num(t.read_measure_ch)),
    ));
    for (i, step) in display_steps(t).iter().enumerate() {
        light_pairs.push((
            format!("text-display-{}", i + 1),
            format!("{}px", fmt_num(*step)),
        ));
    }
    light_pairs.push(("leading-display".to_string(), fmt_num(t.display_leading)));
    let sp = &base.space;
    light_pairs.push(("space-1".to_string(), format!("{}px", fmt_num(sp.s1))));
    light_pairs.push(("space-2".to_string(), format!("{}px", fmt_num(sp.s2))));
    light_pairs.push(("space-3".to_string(), format!("{}px", fmt_num(sp.s3))));
    light_pairs.push(("space-4".to_string(), format!("{}px", fmt_num(sp.s4))));
    light_pairs.push(("space-6".to_string(), format!("{}px", fmt_num(sp.s6))));
    light_pairs.push(("space-8".to_string(), format!("{}px", fmt_num(sp.s8))));
    light_pairs.push(("space-12".to_string(), format!("{}px", fmt_num(sp.s12))));
    let rad = &base.radius;
    light_pairs.push(("radius-sm".to_string(), format!("{}px", fmt_num(rad.sm))));
    light_pairs.push(("radius-md".to_string(), format!("{}px", fmt_num(rad.md))));
    light_pairs.push(("radius-lg".to_string(), format!("{}px", fmt_num(rad.lg))));
    if let Some(xl) = radius_override.and_then(|r| r.xl) {
        light_pairs.push(("radius-xl".to_string(), format!("{}px", fmt_num(xl))));
    }
    light_pairs.push((
        "radius-pill".to_string(),
        format!("{}px", fmt_num(rad.pill)),
    ));
    // Kaliber has no shadow: `--shadow-raise` is `none` unless the app's own
    // `overrides.shadow_raise` supplies layers (colour from base.json).
    let light_shadow = match shadow_override.and_then(|s| s.light.as_ref()) {
        Some(layers) => shadow_css_layers(layers, &base.shadow.color.light),
        None => "none".to_string(),
    };
    light_pairs.push(("shadow-raise".to_string(), light_shadow));
    let disp = &t.display;
    light_pairs.push((
        "text-display".to_string(),
        format!(
            "clamp({}px, {}vw, {}px)",
            fmt_num(disp.min),
            fmt_num(disp.vw),
            fmt_num(disp.max)
        ),
    ));
    let m = &base.motion;
    let motion_base = motion_override.and_then(|mo| mo.base).unwrap_or(m.base);
    light_pairs.push(("motion-fast".to_string(), format!("{}ms", fmt_num(m.fast))));
    light_pairs.push((
        "motion-base".to_string(),
        format!("{}ms", fmt_num(motion_base)),
    ));
    light_pairs.push(("motion-slow".to_string(), format!("{}ms", fmt_num(m.slow))));
    light_pairs.push(("motion-draw".to_string(), format!("{}ms", fmt_num(m.draw))));
    light_pairs.push(("motion-live".to_string(), format!("{}ms", fmt_num(m.live))));
    light_pairs.push(("ease-standard".to_string(), m.ease_standard.clone()));
    light_pairs.push(("ease-loop".to_string(), m.ease_loop.clone()));
    light_pairs.push((
        "motion-offset-press".to_string(),
        format!("{}px", fmt_num(m.offset_press)),
    ));
    light_pairs.push((
        "motion-offset-enter".to_string(),
        format!("{}px", fmt_num(m.offset_enter)),
    ));
    light_pairs.push((
        "motion-offset-focus".to_string(),
        format!("{}px", fmt_num(m.offset_focus)),
    ));
    light_pairs.push(("motion-scale-enter".to_string(), fmt_num(m.scale_enter)));
    light_pairs.push(("motion-scale-mark".to_string(), fmt_num(m.scale_mark)));
    light_pairs.push((
        "motion-tooltip-delay".to_string(),
        format!("{}ms", fmt_num(m.tooltip_delay)),
    ));
    light_pairs.push((
        "motion-ink-hold".to_string(),
        format!("{}ms", fmt_num(m.ink_hold)),
    ));
    light_pairs.push((
        "motion-ink-cap-per-second".to_string(),
        m.ink_cap_per_second.to_string(),
    ));
    light_pairs.push((
        "motion-sequence-step".to_string(),
        format!("{}ms", fmt_num(m.sequence_step)),
    ));
    light_pairs.push((
        "motion-sequence-max".to_string(),
        format!("{}ms", fmt_num(m.sequence_max)),
    ));
    light_pairs.push((
        "motion-sequence-moves".to_string(),
        m.sequence_moves.to_string(),
    ));
    light_pairs.push((
        "control-h".to_string(),
        format!("{}px", fmt_num(base.misc.control_h)),
    ));
    light_pairs.push((
        "control-h-touch".to_string(),
        format!("{}px", fmt_num(base.control.touch)),
    ));

    // New per-app tokens with no base.json formula (plan 098, item C1):
    // theme-invariant, emitted once here (light block), inherited by the
    // dark block through the CSS cascade, same as the scale above.
    if let Some(nt) = new_tokens {
        if let Some(brand) = &nt.accent_brand {
            light_pairs.push((
                "color-accent-brand".to_string(),
                Cv::new(brand.l, brand.c, brand.h, 1.0).css(),
            ));
        }
        if let Some(selected) = &nt.surface_selected {
            light_pairs.push(("color-surface-selected".to_string(), selected.clone()));
        }
    }

    let mut dark_pairs = color_pairs(dark, accent_text.map(|at| &at.dark), own_surface_selected);
    // The dark block only redeclares `--shadow-raise` when the app's own
    // layers differ per theme; otherwise it inherits `none` from the light block.
    if let Some(layers) = shadow_override.and_then(|s| s.dark.as_ref()) {
        dark_pairs.push((
            "shadow-raise".to_string(),
            shadow_css_layers(layers, &base.shadow.color.dark),
        ));
    }

    (light_pairs, dark_pairs)
}

/// The three display steps (plan 125, spec R26: 40, 52 and 68 px), checked here
/// so the CSS and the Rust array cannot disagree on their count.
fn display_steps(t: &schema::TypeScale) -> [f64; 3] {
    match t.display_steps.as_slice() {
        [a, b, c] => [*a, *b, *c],
        other => panic!(
            "design-tokens schema gate: type.display_steps must have exactly 3 steps, got {}",
            other.len()
        ),
    }
}

/// Renders `(name, value)` array literals for both modes, to bake into
/// `OUT_DIR/css_render.rs`.
fn format_pairs_array(pairs: &[(String, String)]) -> String {
    let mut out = String::from("&[");
    for (name, value) in pairs {
        out.push_str(&format!("({name:?}, {value:?}), "));
    }
    out.push(']');
    out
}

/// A `stroke-dasharray` value: the dash lengths in px separated by spaces,
/// or `none` for a solid line (an empty list).
fn dash_css(dash: &[f64]) -> String {
    if dash.is_empty() {
        "none".to_string()
    } else {
        dash.iter()
            .map(|d| fmt_num(*d))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// A multi-layer shadow override (plan 098, item C1): each layer keeps the
/// theme's own base shadow colour (l/c/h from base.json's
/// `shadow.color.<theme>`); only geometry and alpha are an app's choice.
/// Layers are joined `", "`, the same separator a hand-authored multi-layer
/// `box-shadow` uses.
fn shadow_css_layers(layers: &[ShadowLayerOverride], color: &ColorSpec) -> String {
    layers
        .iter()
        .map(|layer| {
            let cv = Cv::new(color.l, color.c, color.h, layer.alpha);
            format!(
                "{} {} {} {}",
                len_px(layer.x.unwrap_or(0.0)),
                len_px(layer.y),
                len_px(layer.blur),
                cv.css()
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// A CSS length in px, except a value that rounds to exactly zero renders
/// as bare `0` (no unit): the `0 1px 4px oklch(...)` shape of a
/// hand-written `box-shadow` (controller fix round 1, minor item 3).
fn len_px(x: f64) -> String {
    let n = fmt_num(x);
    if n == "0" {
        n
    } else {
        format!("{n}px")
    }
}

fn color_pairs(
    t: &ThemeValues,
    accent_text: Option<&schema::LiteralColor>,
    own_surface_selected: bool,
) -> CssPairs {
    let mut out = vec![
        ("color-background".to_string(), t.background.css()),
        ("color-well".to_string(), t.well.css()),
        ("color-surface-raised".to_string(), t.surface_raised.css()),
        ("color-surface-hover".to_string(), t.surface_hover.css()),
        ("color-surface-active".to_string(), t.surface_active.css()),
        ("color-border".to_string(), t.border.css()),
        ("color-border-strong".to_string(), t.border_strong.css()),
        ("color-control".to_string(), t.control.css()),
        ("color-text".to_string(), t.text.css()),
        ("color-text-muted".to_string(), t.text_muted.css()),
        ("color-disabled-text".to_string(), t.disabled_text.css()),
        ("color-disabled-fill".to_string(), t.disabled_fill.css()),
        ("color-scrim".to_string(), t.scrim.css()),
        ("color-ink".to_string(), t.ink.fill.css()),
        ("color-on-ink".to_string(), t.ink.text.css()),
        ("color-on-ink-muted".to_string(), t.ink.muted.css()),
        ("color-ink-rule".to_string(), t.ink.rule.css()),
        ("color-ink-action".to_string(), t.ink.action.css()),
        ("color-on-ink-action".to_string(), t.ink.on_action.css()),
        ("color-accent".to_string(), t.accent.css()),
        ("color-accent-hover".to_string(), t.accent_hover.css()),
        ("color-accent-active".to_string(), t.accent_active.css()),
        ("color-accent-soft".to_string(), t.accent_soft.css()),
        (
            "color-accent-soft-active".to_string(),
            t.accent_soft_active.css(),
        ),
        (
            "color-accent-soft-foreground".to_string(),
            t.accent_soft_foreground.css(),
        ),
        ("color-focus".to_string(), t.focus.css()),
        ("color-on-accent".to_string(), t.on_accent.css()),
    ];
    // The selected-row fill, accent-derived (direction A). An app that ships
    // its own literal `surface_selected` override skips it (see the caller).
    if !own_surface_selected {
        out.insert(
            out.len() - 2,
            (
                "color-surface-selected".to_string(),
                t.surface_selected.css(),
            ),
        );
    }
    // `accent-text` (plan 098, item C1): a brand-new per-theme token, no
    // base.json formula, right after `on-accent` in the target's own field
    // order.
    if let Some(lit) = accent_text {
        out.push((
            "color-accent-text".to_string(),
            Cv::new(lit.l, lit.c, lit.h, 1.0).css(),
        ));
    }
    out.extend(vec![
        ("color-danger".to_string(), t.danger.css()),
        ("color-danger-hover".to_string(), t.danger_hover.css()),
        ("color-danger-active".to_string(), t.danger_active.css()),
        ("color-danger-soft".to_string(), t.danger_soft.css()),
        ("color-warning".to_string(), t.warning.css()),
        ("color-warning-soft".to_string(), t.warning_soft.css()),
        ("color-success".to_string(), t.success.css()),
        ("color-success-soft".to_string(), t.success_soft.css()),
    ]);
    for (i, c) in t.chart.iter().enumerate() {
        out.push((format!("color-chart-{}", i + 1), c.css()));
    }
    out.push((
        "color-chart-band-alpha".to_string(),
        fmt_num(t.chart_band_alpha),
    ));
    out.push(("color-chart-grid".to_string(), t.chart_grid.css()));
    for ea in &t.extras_accent {
        out.push((format!("color-{}", ea.name), ea.accent.css()));
        out.push((format!("color-{}-hover", ea.name), ea.hover.css()));
        out.push((format!("color-{}-active", ea.name), ea.active.css()));
        out.push((format!("color-{}-soft", ea.name), ea.soft.css()));
        out.push((
            format!("color-{}-soft-foreground", ea.name),
            ea.soft_foreground.css(),
        ));
    }
    for es in &t.extras_series {
        out.push((format!("color-{}", es.name), es.value.css()));
    }
    if let Some(a2) = &t.accent2 {
        out.push(("color-accent2-field".to_string(), a2.field.css()));
        out.push(("color-accent2-highlight".to_string(), a2.highlight.css()));
        out.push(("color-accent2-mark".to_string(), a2.mark.css()));
    }
    out
}

/// Theme-invariant scale consts (space/radius/type/motion/control-h). Kaliber
/// has no shadow, so there is no shadow const.
fn render_scale(base: &BaseTokens) -> String {
    let space = &base.space;
    let radius = &base.radius;
    let t = &base.type_scale;
    let m = &base.motion;
    let steps = display_steps(t);

    let mut out = String::new();
    out.push_str("#[derive(Clone, Copy, Debug)]\n");
    out.push_str("pub struct Space { pub s1: f32, pub s2: f32, pub s3: f32, pub s4: f32, pub s6: f32, pub s8: f32, pub s12: f32 }\n");
    out.push_str("#[derive(Clone, Copy, Debug)]\n");
    out.push_str("pub struct Radius { pub sm: f32, pub md: f32, pub lg: f32, pub pill: f32 }\n");
    out.push_str("#[derive(Clone, Copy, Debug)]\n");
    out.push_str("pub struct Display { pub min: f32, pub vw: f32, pub max: f32 }\n");
    out.push_str("#[derive(Clone, Copy, Debug)]\n");
    out.push_str("pub struct TypeScale { pub xs: f32, pub sm: f32, pub body: f32, pub md: f32, pub lg: f32, pub xl: f32, pub xxl: f32, pub xxxl: f32, pub read: f32, pub read_leading: f32, pub read_measure_ch: f32, pub display_steps: [f32; 3], pub display_leading: f32, pub font_sans: &'static str, pub numerals: &'static str, pub display: Display }\n");
    out.push_str("#[derive(Clone, Copy, Debug)]\n");
    out.push_str("pub struct Motion { pub fast: f32, pub base: f32, pub slow: f32, pub draw: f32, pub live: f32, pub ease_standard: &'static str, pub ease_loop: &'static str, pub offset_press: f32, pub offset_enter: f32, pub offset_focus: f32, pub scale_enter: f32, pub scale_mark: f32, pub tooltip_delay: f32, pub ink_hold: f32, pub ink_cap_per_second: u32, pub sequence_step: f32, pub sequence_max: f32, pub sequence_moves: u32 }\n");
    out.push_str("#[derive(Clone, Copy, Debug)]\n");
    out.push_str("pub struct Control { pub touch: f32 }\n\n");

    let _ = writeln!(
        out,
        "pub static SPACE: Space = Space {{ s1: {:?}f32, s2: {:?}f32, s3: {:?}f32, s4: {:?}f32, s6: {:?}f32, s8: {:?}f32, s12: {:?}f32 }};",
        space.s1 as f32, space.s2 as f32, space.s3 as f32, space.s4 as f32, space.s6 as f32, space.s8 as f32, space.s12 as f32
    );
    let _ = writeln!(
        out,
        "pub static RADIUS: Radius = Radius {{ sm: {:?}f32, md: {:?}f32, lg: {:?}f32, pill: {:?}f32 }};",
        radius.sm as f32, radius.md as f32, radius.lg as f32, radius.pill as f32
    );
    let _ = writeln!(
        out,
        "pub static TYPE: TypeScale = TypeScale {{ xs: {:?}f32, sm: {:?}f32, body: {:?}f32, md: {:?}f32, lg: {:?}f32, xl: {:?}f32, xxl: {:?}f32, xxxl: {:?}f32, read: {:?}f32, read_leading: {:?}f32, read_measure_ch: {:?}f32, display_steps: [{:?}f32, {:?}f32, {:?}f32], display_leading: {:?}f32, font_sans: {:?}, numerals: {:?}, display: Display {{ min: {:?}f32, vw: {:?}f32, max: {:?}f32 }} }};",
        t.xs as f32, t.sm as f32, t.body as f32, t.md as f32, t.lg as f32, t.xl as f32, t.xxl as f32, t.xxxl as f32, t.read as f32, t.read_leading as f32, t.read_measure_ch as f32, steps[0] as f32, steps[1] as f32, steps[2] as f32, t.display_leading as f32, t.font_sans, t.numerals,
        t.display.min as f32, t.display.vw as f32, t.display.max as f32
    );
    let _ = writeln!(
        out,
        "pub static MOTION: Motion = Motion {{ fast: {:?}f32, base: {:?}f32, slow: {:?}f32, draw: {:?}f32, live: {:?}f32, ease_standard: {:?}, ease_loop: {:?}, offset_press: {:?}f32, offset_enter: {:?}f32, offset_focus: {:?}f32, scale_enter: {:?}f32, scale_mark: {:?}f32, tooltip_delay: {:?}f32, ink_hold: {:?}f32, ink_cap_per_second: {}, sequence_step: {:?}f32, sequence_max: {:?}f32, sequence_moves: {} }};",
        m.fast as f32, m.base as f32, m.slow as f32, m.draw as f32, m.live as f32, m.ease_standard, m.ease_loop,
        m.offset_press as f32, m.offset_enter as f32, m.offset_focus as f32, m.scale_enter as f32, m.scale_mark as f32,
        m.tooltip_delay as f32, m.ink_hold as f32, m.ink_cap_per_second, m.sequence_step as f32, m.sequence_max as f32, m.sequence_moves
    );
    let _ = writeln!(
        out,
        "pub const CONTROL_H: f32 = {:?}f32;",
        base.misc.control_h as f32
    );
    let _ = writeln!(
        out,
        "pub static CONTROL: Control = Control {{ touch: {:?}f32 }};",
        base.control.touch as f32
    );

    // The six chart stroke patterns, in series order (series i uses pattern i).
    assert_eq!(
        base.dataviz.stroke_patterns.len(),
        6,
        "design-tokens schema gate: dataviz.stroke_patterns must have exactly 6 patterns, got {}",
        base.dataviz.stroke_patterns.len()
    );
    out.push_str("#[derive(Clone, Copy, Debug)]\n");
    out.push_str("pub struct StrokePattern { pub name: &'static str, pub dash: &'static [f32] }\n");
    let patterns: Vec<String> = base
        .dataviz
        .stroke_patterns
        .iter()
        .map(|p| {
            let dash: Vec<String> = p
                .dash
                .iter()
                .map(|d| format!("{:?}f32", *d as f32))
                .collect();
            format!(
                "StrokePattern {{ name: {:?}, dash: &[{}] }}",
                p.name,
                dash.join(", ")
            )
        })
        .collect();
    let _ = writeln!(
        out,
        "pub static STROKE_PATTERNS: [StrokePattern; 6] = [{}];",
        patterns.join(", ")
    );

    out
}
