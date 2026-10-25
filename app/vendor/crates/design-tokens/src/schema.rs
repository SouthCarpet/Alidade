// Strict (deny-unknown-fields) mirrors of `design/tokens/schema.json`.
//
// This file is the schema gate: any token JSON with a typo'd or extra key
// fails to deserialize, and the caller (`build.rs`) turns that into a
// build failure with a clear message. It is included two ways so there is
// exactly one definition:
// - `build.rs` pulls it in with `include!("src/schema.rs")` (build.rs is
//   its own compilation, outside the crate's module tree).
// - `src/lib.rs` declares `#[cfg(test)] mod schema;`, which is ordinary
//   Rust module resolution — that's what lets the negative-build-check
//   unit test below run under `cargo test -p design-tokens` without the
//   shipped library depending on serde at all.

use serde::Deserialize;

fn default_alpha() -> f64 {
    1.0
}

#[derive(Deserialize, Clone, Copy, Debug)]
#[serde(deny_unknown_fields)]
pub struct ColorSpec {
    pub l: f64,
    pub c: f64,
    pub h: f64,
    #[serde(default = "default_alpha")]
    pub a: f64,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct NeutralSet {
    pub background: ColorSpec,
    pub well: ColorSpec,
    #[serde(rename = "surface-raised")]
    pub surface_raised: ColorSpec,
    #[serde(rename = "surface-hover")]
    pub surface_hover: ColorSpec,
    /// The pressed tone step (plan 125, spec R25): at least ΔL 0.08 from the sheet.
    #[serde(rename = "surface-active")]
    pub surface_active: ColorSpec,
    pub border: ColorSpec,
    #[serde(rename = "border-strong")]
    pub border_strong: ColorSpec,
    pub control: ColorSpec,
    pub text: ColorSpec,
    #[serde(rename = "text-muted")]
    pub text_muted: ColorSpec,
    /// Disabled text (plan 125, spec R25): at least 3:1 on every surface, with a
    /// non-colour sign beside it.
    #[serde(rename = "disabled-text")]
    pub disabled_text: ColorSpec,
    #[serde(rename = "disabled-fill")]
    pub disabled_fill: ColorSpec,
    pub scrim: ColorSpec,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ThemedNeutrals {
    pub light: NeutralSet,
    pub dark: NeutralSet,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AccentLc {
    pub l: f64,
    pub c: f64,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Delta {
    pub l_delta: f64,
    #[serde(default)]
    pub c_delta: f64,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Soft {
    pub l: f64,
    pub c_factor: f64,
    pub c_max: f64,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct SoftActive {
    #[serde(default)]
    pub l_delta: f64,
    pub c_delta: f64,
}

/// The selected-row fill (direction A): lightness `l`, chroma
/// `accent.c * c_factor`, at the accent hue.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Selected {
    pub l: f64,
    pub c_factor: f64,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct SoftForeground {
    pub l: f64,
    pub c_factor: f64,
    pub min_contrast: f64,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Focus {
    pub l: f64,
    #[serde(default)]
    pub c: Option<f64>,
    pub ring_width_px: f64,
    pub ring_offset_px: f64,
    pub min_contrast: f64,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct OnAccent {
    pub l: f64,
    pub c: f64,
    pub h: f64,
}

/// The chroma rule of one second-hue role (plan 125, lane k12): C =
/// min(c_max, c_factor x the primary chroma of the matching role).
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Accent2Chroma {
    pub c_factor: f64,
    pub c_max: f64,
}

/// The second-hue tones (spec R35, lane k12): field follows soft, highlight
/// follows selected, mark follows the accent. Lightness comes from soft,
/// selected and `dataviz.series_color`.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Accent2Formula {
    pub field: Accent2Chroma,
    pub highlight: Accent2Chroma,
    pub mark: Accent2Chroma,
}

/// The ink block (plan 125, spec R25): one dark block per page, dark in both
/// themes, every part an `{l, c}` at the accent hue.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct InkFormula {
    pub fill: AccentLc,
    pub text: AccentLc,
    pub muted: AccentLc,
    pub rule: AccentLc,
    pub action: AccentLc,
    pub on_action: AccentLc,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AccentFamilyTheme {
    pub accent: AccentLc,
    pub hover: Delta,
    pub active: Delta,
    pub soft: Soft,
    pub soft_active: SoftActive,
    pub soft_foreground: SoftForeground,
    pub selected: Selected,
    pub focus: Focus,
    pub on_accent: OnAccent,
    pub ink: InkFormula,
    pub accent2: Accent2Formula,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AccentFamily {
    #[serde(rename = "_comment", default)]
    pub comment: Option<String>,
    pub light: AccentFamilyTheme,
    pub dark: AccentFamilyTheme,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct StatusSet {
    pub danger: ColorSpec,
    pub danger_hover: ColorSpec,
    pub danger_active: ColorSpec,
    pub danger_soft: ColorSpec,
    pub warning: ColorSpec,
    pub warning_soft: ColorSpec,
    pub success: ColorSpec,
    pub success_soft: ColorSpec,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ThemedStatus {
    pub light: StatusSet,
    pub dark: StatusSet,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct SeriesColor {
    pub l: f64,
    pub c: f64,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ThemedSeriesColor {
    pub light: SeriesColor,
    pub dark: SeriesColor,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ThemedAlpha {
    pub light: f64,
    pub dark: f64,
}

/// One chart stroke pattern: its name and the dash lengths in px (an empty
/// list is a solid line). Series `i` (1-based) uses pattern `i`; the six
/// patterns are the chart's identity device (direction A, one-hue ramp).
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct StrokePattern {
    pub name: String,
    pub dash: Vec<f64>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ThemedRamp {
    pub light: Vec<SeriesColor>,
    pub dark: Vec<SeriesColor>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Dataviz {
    #[serde(rename = "_comment", default)]
    pub comment: Option<String>,
    pub ramp: ThemedRamp,
    pub series_color: ThemedSeriesColor,
    pub accent_exclusion_deg: f64,
    pub band_alpha: f64,
    pub grid_alpha: ThemedAlpha,
    pub history_chroma_factor: f64,
    pub stroke_patterns: Vec<StrokePattern>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct SpaceScale {
    #[serde(rename = "1")]
    pub s1: f64,
    #[serde(rename = "2")]
    pub s2: f64,
    #[serde(rename = "3")]
    pub s3: f64,
    #[serde(rename = "4")]
    pub s4: f64,
    #[serde(rename = "6")]
    pub s6: f64,
    #[serde(rename = "8")]
    pub s8: f64,
    #[serde(rename = "12")]
    pub s12: f64,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RadiusScale {
    pub sm: f64,
    pub md: f64,
    pub lg: f64,
    pub pill: f64,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct DisplaySpec {
    pub min: f64,
    pub vw: f64,
    pub max: f64,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct TypeScale {
    pub xs: f64,
    pub sm: f64,
    pub body: f64,
    pub md: f64,
    pub lg: f64,
    pub xl: f64,
    #[serde(rename = "2xl")]
    pub xxl: f64,
    #[serde(rename = "3xl")]
    pub xxxl: f64,
    /// Running text (plan 125, spec R26): size in px, line height, column width in ch.
    pub read: f64,
    pub read_leading: f64,
    pub read_measure_ch: f64,
    /// The three display steps in px (40, 52, 68), weight 700, at `display_leading`.
    pub display_steps: Vec<f64>,
    pub display_leading: f64,
    pub font_sans: String,
    pub numerals: String,
    pub display: DisplaySpec,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct MotionScale {
    #[serde(rename = "_comment", default)]
    pub comment: Option<String>,
    pub fast: f64,
    pub base: f64,
    pub slow: f64,
    pub draw: f64,
    pub live: f64,
    pub ease_standard: String,
    pub ease_loop: String,
    pub offset_press: f64,
    pub offset_enter: f64,
    pub offset_focus: f64,
    pub scale_enter: f64,
    pub scale_mark: f64,
    pub tooltip_delay: f64,
    pub ink_hold: f64,
    /// A whole count of ink changes per second in one view (lane K6), not a time.
    pub ink_cap_per_second: u32,
    /// Cause then effect (plan 125, spec R32): ms between related moves after one
    /// action, the ms cap of the whole sequence and the move count cap.
    pub sequence_step: f64,
    pub sequence_max: f64,
    pub sequence_moves: u32,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ThemedShadowColor {
    pub light: ColorSpec,
    pub dark: ColorSpec,
}

/// Kaliber has no shadow of its own (`--shadow-raise` is `none`). The block
/// only carries the colour an app's `overrides.shadow_raise` layers use.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ShadowBlock {
    #[serde(rename = "_comment", default)]
    pub comment: Option<String>,
    pub color: ThemedShadowColor,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct MiscBlock {
    pub control_h: f64,
}

/// `control.touch` (plan 094 Task 8, item L): a taller control-height mode
/// beside `misc.control_h` -- a mode a caller opts into, not a viewport
/// breakpoint.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ControlBlock {
    pub touch: f64,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct BaseTokens {
    pub version: u32,
    pub neutrals: ThemedNeutrals,
    pub accent_family: AccentFamily,
    pub status: ThemedStatus,
    pub dataviz: Dataviz,
    pub space: SpaceScale,
    pub radius: RadiusScale,
    #[serde(rename = "type")]
    pub type_scale: TypeScale,
    pub motion: MotionScale,
    pub shadow: ShadowBlock,
    pub misc: MiscBlock,
    pub control: ControlBlock,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct NeutralTint {
    pub hue: f64,
    pub chroma: f64,
}

/// One named identity extra (plan 094 Task 9, item F; replaces the old
/// `second_accent`/`team_rings` fields). `formula: "accent"` reuses the
/// base accent_family's hover/active/soft/soft_foreground formulas verbatim
/// at `hue` -- `name` becomes the CSS custom-property stem, e.g. `hextech`
/// -> `--color-hextech`, `--color-hextech-hover`, ...
/// (the muted `-rail` variant left in plan 094 revision 2: identity rails are
/// an AI-slop block device). `formula:
/// "series"` emits one flat colour from the shared dataviz series_color
/// l/c formula at `hue` (e.g. `team-blue` -> `--color-team-blue`). `name`
/// must be kebab-case; build.rs refuses the reserved stems `accent`,
/// `danger`, `warning`, `success`, `chart` (the `formula` string itself is
/// validated by build.rs, not serde, so an unknown formula gets one gate
/// message instead of a generic deserialize error).
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Extra {
    pub name: String,
    pub formula: String,
    pub hue: f64,
}

/// One accepted hue collision (item D, D10): silences the build.rs hue gate
/// for exactly this pair. `with` names a status ("danger"/"warning"/
/// "success") or another app's kebab name; `reason` must be non-empty
/// (build.rs enforces this, serde cannot) and names the date and the
/// decision owner, since D10 makes every hue change Michal's call.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AcceptedHueOverlap {
    pub with: String,
    pub reason: String,
}

/// A partial OKLCH colour patch (plan 098, item C1): any of `l`/`c`/`h`
/// present replaces the corresponding component of an already-computed
/// value; an absent component keeps what the base formula produced.
#[derive(Deserialize, Debug, Default, Clone, Copy)]
#[serde(deny_unknown_fields)]
pub struct ColorPatch {
    #[serde(default)]
    pub l: Option<f64>,
    #[serde(default)]
    pub c: Option<f64>,
    #[serde(default)]
    pub h: Option<f64>,
}

/// A full literal OKLCH colour (plan 098, item C1): all three components
/// required. Used for a brand-new per-app token with no base.json formula
/// to patch (`new_tokens`, below).
#[derive(Deserialize, Debug, Clone, Copy)]
#[serde(deny_unknown_fields)]
pub struct LiteralColor {
    pub l: f64,
    pub c: f64,
    pub h: f64,
}

/// Per-theme colour overrides (plan 098, item C1). `accent` patches the
/// accent-family formula's own l/c/h input BEFORE hover/active/soft/
/// soft_active/soft_foreground/focus's chroma are derived from it, so the
/// existing base.json deltas cascade unchanged -- only the input changes,
/// never the formula, and an app's `extras` stay on the base input (never
/// this override). `focus_l` patches the focus ring's own lightness (not
/// accent-derived). `background`/`surface_raised`/`surface_hover`/
/// `border`/`on_accent` each patch that already-computed neutral value
/// directly. `surface_hover` is authored in base.json (direction A), so a
/// `surface_raised` patch does not move it: patch `surface_hover` as well.
/// `accent` also moves the accent-derived `surface_selected`.
#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct ThemeOverride {
    #[serde(default)]
    pub accent: Option<ColorPatch>,
    #[serde(default)]
    pub focus_l: Option<f64>,
    #[serde(default)]
    pub background: Option<ColorPatch>,
    #[serde(default)]
    pub surface_raised: Option<ColorPatch>,
    #[serde(default)]
    pub surface_hover: Option<ColorPatch>,
    #[serde(default)]
    pub border: Option<ColorPatch>,
    #[serde(default)]
    pub on_accent: Option<ColorPatch>,
}

/// One `formula: "accent"` extra's per-theme override (plan 098, item C1):
/// only the `soft` sub-value is supported today, the one shipped need.
/// Extend with another field the day another app needs to patch a
/// different extra sub-value.
#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct ExtraOverrideField {
    #[serde(default)]
    pub soft: Option<ColorPatch>,
}

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct ExtraThemeOverrides {
    #[serde(default)]
    pub light: Option<ExtraOverrideField>,
    #[serde(default)]
    pub dark: Option<ExtraOverrideField>,
}

/// One shadow layer override (plan 098, item C1): `x` defaults to 0 (every
/// shipped shadow is a pure vertical offset); `y`/`blur`/`alpha` are
/// required. Colour stays the theme's own `shadow.color.<theme>` l/c/h from
/// base.json -- only the geometry and alpha are an app choice.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ShadowLayerOverride {
    #[serde(default)]
    pub x: Option<f64>,
    pub y: f64,
    pub blur: f64,
    pub alpha: f64,
}

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct ShadowRaiseOverride {
    #[serde(default)]
    pub light: Option<Vec<ShadowLayerOverride>>,
    #[serde(default)]
    pub dark: Option<Vec<ShadowLayerOverride>>,
}

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct MotionOverride {
    #[serde(default)]
    pub base: Option<f64>,
}

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct RadiusOverride {
    #[serde(default)]
    pub xl: Option<f64>,
}

/// A brand-new per-app token with no base.json formula at all (plan 098,
/// item C1): `accent_text` needs its own colour per theme; `accent_brand`
/// and `surface_selected` are theme-invariant, emitted once in the light
/// block, inherited by the dark block through the CSS cascade (the same
/// pattern the shared scale already uses). `surface_selected` is a literal
/// CSS string, the one exception to the l/c/h vocabulary: a `color-mix()`
/// expression over two `var()` references has no OKLCH triple to hold it.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AccentTextOverride {
    pub light: LiteralColor,
    pub dark: LiteralColor,
}

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct NewTokensOverride {
    #[serde(default)]
    pub accent_text: Option<AccentTextOverride>,
    #[serde(default)]
    pub accent_brand: Option<LiteralColor>,
    #[serde(default)]
    pub surface_selected: Option<String>,
}

/// One app's full override set (plan 098, item C1): everything an app can
/// change without touching base.json (base.json values never change).
/// `extras` is keyed by the extra's own `name` (already validated
/// kebab-case elsewhere), open-ended like `extras` itself.
#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct Overrides {
    #[serde(rename = "_comment", default)]
    pub comment: Option<String>,
    #[serde(default)]
    pub light: Option<ThemeOverride>,
    #[serde(default)]
    pub dark: Option<ThemeOverride>,
    #[serde(default)]
    pub motion: Option<MotionOverride>,
    #[serde(default)]
    pub radius: Option<RadiusOverride>,
    #[serde(default)]
    pub extras: std::collections::HashMap<String, ExtraThemeOverrides>,
    #[serde(default)]
    pub shadow_raise: Option<ShadowRaiseOverride>,
    #[serde(default)]
    pub new_tokens: Option<NewTokensOverride>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AppTokens {
    pub name: String,
    pub accent_hue: f64,
    /// The optional second identity hue (plan 125, spec R35), for tones only:
    /// accent2-field, accent2-highlight and accent2-mark. Absent: one hue.
    #[serde(default)]
    pub accent2_hue: Option<f64>,
    /// Absent: the app takes `base.json`'s neutrals as authored (hue 200,
    /// per-role chroma). Present: every tinted neutral takes this hue and
    /// chroma, same lightness bands.
    #[serde(default)]
    pub neutral_tint: Option<NeutralTint>,
    #[serde(default)]
    pub provisional: Option<bool>,
    #[serde(default)]
    pub mark: Option<String>,
    #[serde(default)]
    pub extras: Vec<Extra>,
    #[serde(default)]
    pub accepted_hue_overlaps: Vec<AcceptedHueOverlap>,
    #[serde(default)]
    pub overrides: Option<Overrides>,
}

pub fn parse_base(text: &str) -> Result<BaseTokens, serde_json::Error> {
    serde_json::from_str(text)
}

pub fn parse_app(text: &str) -> Result<AppTokens, serde_json::Error> {
    serde_json::from_str(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// Step 5 of the task brief: prove the schema gate actually rejects an
    /// invalid file, without ever touching the real tokens. Copies the real
    /// base.json into target/tmp-bad/, injects one unknown top-level key,
    /// and asserts `parse_base` errors (via #[should_panic] on .unwrap()).
    #[test]
    #[should_panic(expected = "unknown field")]
    fn rejects_unknown_field_in_a_copy_of_base_json() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let real_base = PathBuf::from(manifest_dir).join("../../tokens/base.json");
        let text = fs::read_to_string(&real_base)
            .expect("read the real base.json to build the negative-test copy from");

        let mut value: serde_json::Value =
            serde_json::from_str(&text).expect("real base.json must itself be valid JSON");
        value["totally_unknown_field"] = serde_json::json!("this key is not in the schema");
        let bad_text = serde_json::to_string_pretty(&value).unwrap();

        let bad_dir = PathBuf::from(manifest_dir).join("../../target/tmp-bad");
        fs::create_dir_all(&bad_dir).expect("create the target/tmp-bad scratch dir");
        let bad_path = bad_dir.join("base-bad.json");
        fs::write(&bad_path, &bad_text).expect("write the bad copy");

        let bad_text_read = fs::read_to_string(&bad_path).expect("read back the bad copy");
        // The real base.json on disk is untouched — only base-bad.json
        // under target/tmp-bad/ (gitignored) was written.
        parse_base(&bad_text_read).unwrap();
    }

    /// Plan 098, item C1: an unknown key inside `overrides` fails the same
    /// schema gate as an unknown top-level key, on a copy of the first real
    /// app file in `tokens/apps/` (never the file on disk). Any app file will
    /// do, so a vendored copy with one app file runs it too: indexing a missing
    /// `overrides` adds the block, and the unknown key is rejected the same way.
    #[test]
    #[should_panic(expected = "unknown field")]
    fn rejects_unknown_field_in_a_copy_of_an_app_overrides() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let apps_dir = PathBuf::from(manifest_dir).join("../../tokens/apps");
        let mut app_files: Vec<PathBuf> = fs::read_dir(&apps_dir)
            .expect("read tokens/apps to pick a negative-test source")
            .map(|entry| entry.expect("read a tokens/apps entry").path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect();
        app_files.sort();
        let real_app = app_files.first().expect("tokens/apps holds an app file");
        let text = fs::read_to_string(real_app)
            .expect("read the real app file to build the negative-test copy from");

        let mut value: serde_json::Value =
            serde_json::from_str(&text).expect("real app file must itself be valid JSON");
        value["overrides"]["light"]["totally_unknown_field"] = serde_json::json!(1.0);
        let bad_text = serde_json::to_string_pretty(&value).unwrap();

        let bad_dir = PathBuf::from(manifest_dir).join("../../target/tmp-bad");
        fs::create_dir_all(&bad_dir).expect("create the target/tmp-bad scratch dir");
        let bad_path = bad_dir.join("app-overrides-bad.json");
        fs::write(&bad_path, &bad_text).expect("write the bad copy");

        let bad_text_read = fs::read_to_string(&bad_path).expect("read back the bad copy");
        // The real app file on disk is untouched: only
        // app-overrides-bad.json under target/tmp-bad/ (gitignored)
        // was written.
        parse_app(&bad_text_read).unwrap();
    }
}
