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
    pub border: ColorSpec,
    #[serde(rename = "border-top")]
    pub border_top: ColorSpec,
    pub milled: ColorSpec,
    pub text: ColorSpec,
    #[serde(rename = "text-muted")]
    pub text_muted: ColorSpec,
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
    pub c_delta: f64,
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

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AccentFamilyTheme {
    pub accent: AccentLc,
    pub hover: Delta,
    pub active: Delta,
    pub soft: Soft,
    pub soft_active: SoftActive,
    pub soft_foreground: SoftForeground,
    pub focus: Focus,
    pub on_accent: OnAccent,
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
pub struct Dataviz {
    #[serde(rename = "_comment", default)]
    pub comment: Option<String>,
    pub series_color: ThemedSeriesColor,
    pub anchor_hues: Vec<f64>,
    pub spare_hue: f64,
    pub accent_exclusion_deg: f64,
    pub anchor_slots: usize,
    pub band_alpha: f64,
    pub grid_alpha: f64,
    pub history_chroma_factor: f64,
    pub stroke_patterns: Vec<String>,
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
pub struct TypeScale {
    pub xs: f64,
    pub sm: f64,
    pub md: f64,
    pub lg: f64,
    pub xl: f64,
    #[serde(rename = "2xl")]
    pub xxl: f64,
    #[serde(rename = "3xl")]
    pub xxxl: f64,
    pub font_sans: String,
    pub numerals: String,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct MotionScale {
    pub fast: f64,
    pub base: f64,
    pub draw: f64,
    pub live: f64,
    pub ease_standard: String,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ShadowSpec {
    pub x: f64,
    pub y: f64,
    pub blur: f64,
    pub color: ColorSpec,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ShadowRaiseBlock {
    pub light: ShadowSpec,
    pub dark: ShadowSpec,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ShadowBlock {
    pub raise: ShadowRaiseBlock,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct MiscBlock {
    pub control_h: f64,
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
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct NeutralTint {
    pub hue: f64,
    pub chroma: f64,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AppTokens {
    pub name: String,
    pub accent_hue: f64,
    pub neutral_tint: NeutralTint,
    #[serde(default)]
    pub series_order: Option<Vec<f64>>,
    #[serde(default)]
    pub provisional: Option<bool>,
    #[serde(default)]
    pub mark: Option<String>,
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

        let mut value: serde_json::Value = serde_json::from_str(&text)
            .expect("real base.json must itself be valid JSON");
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
}
