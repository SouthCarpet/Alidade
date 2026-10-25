// Colour math shared between the crate's runtime API and `build.rs`.
//
// This file is intentionally self-contained (no `crate::` imports, no
// external dependencies) so that `build.rs` can pull it in verbatim with
// `include!("src/color.rs")` and reuse the exact same conversion code that
// ships in the compiled crate — one implementation, two call sites, no
// drift between what generates the constants and what the tests check them
// against.
//
// OKLab <-> linear-sRGB matrices are the ones published in the CSS Color 4
// spec ("Sample code for Color Conversions", oklab-to-linear-srgb), which
// trace back to Björn Ottosson's original OKLab derivation. Verified by
// round-tripping sRGB red (1,0,0) -> OKLab (L 0.627955, a 0.224863,
// b 0.125846) -> back through this module's `oklch_to_srgb` and recovering
// (0.999999, 0.0000000, 0.0000033), i.e. red to well within float noise.

/// A straight (non-premultiplied) sRGB colour with an alpha channel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Rgba { r, g, b, a }
    }
}

/// Convert an OKLCH colour (`l` 0-1, `c` >= 0, `h_deg` 0-360) to sRGB,
/// clamped per-channel to `0.0..=1.0` for out-of-gamut input.
///
/// Source: CSS Color 4 §"Converting Colors" sample code, OKLab -> linear
/// sRGB matrices (Björn Ottosson's OKLab constants), followed by the
/// standard sRGB transfer-function encode (IEC 61966-2-1).
pub fn oklch_to_srgb(l: f64, c: f64, h_deg: f64) -> (f32, f32, f32) {
    let (r_lin, g_lin, b_lin) = oklch_to_linear_srgb(l, c, h_deg);
    (
        clamp01(srgb_encode(r_lin)) as f32,
        clamp01(srgb_encode(g_lin)) as f32,
        clamp01(srgb_encode(b_lin)) as f32,
    )
}

/// True when no linear-sRGB channel of the OKLCH colour leaves `0..=1`, so
/// the per-channel clamp in [`oklch_to_srgb`] changes nothing.
pub fn in_srgb_gamut(l: f64, c: f64, h_deg: f64) -> bool {
    const EPS: f64 = 1e-9;
    let (r, g, b) = oklch_to_linear_srgb(l, c, h_deg);
    [r, g, b].iter().all(|v| (-EPS..=1.0 + EPS).contains(v))
}

/// The chroma to use for `(l, c, h)` so the colour is inside sRGB: `c` when
/// it already is, else the largest chroma at the same L and H that is,
/// floored to 4 decimals (the precision the CSS prints), so the `oklch()`
/// string and the `Rgba` const are the same colour. Chroma reduction at
/// fixed L and H, not channel clipping (CSS Color 4, "Gamut mapping").
pub fn fit_chroma_to_srgb(l: f64, c: f64, h_deg: f64) -> f64 {
    if in_srgb_gamut(l, c, h_deg) {
        return c;
    }
    let (mut lo, mut hi) = (0.0, c);
    for _ in 0..50 {
        let mid = (lo + hi) / 2.0;
        if in_srgb_gamut(l, mid, h_deg) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo * 10000.0).floor() / 10000.0
}

/// The chroma of one second-hue tone (spec R35, lane k12): `c_factor` times
/// the primary chroma of the same role, capped at `c_max`, then fitted into
/// sRGB at fixed L and H. build.rs derives every `--color-accent2-*` with it.
pub fn accent2_chroma(l: f64, primary_c: f64, c_factor: f64, c_max: f64, h_deg: f64) -> f64 {
    fit_chroma_to_srgb(l, c_max.min(c_factor * primary_c), h_deg)
}

fn oklch_to_linear_srgb(l: f64, c: f64, h_deg: f64) -> (f64, f64, f64) {
    let h = h_deg.to_radians();
    let a = c * h.cos();
    let b = c * h.sin();

    // OKLab -> LMS (nonlinear)
    let l_ = l + 0.3963377774 * a + 0.2158037573 * b;
    let m_ = l - 0.1055613458 * a - 0.0638541728 * b;
    let s_ = l - 0.0894841775 * a - 1.2914855480 * b;

    // undo the cube-root used when going the other direction
    let l3 = l_ * l_ * l_;
    let m3 = m_ * m_ * m_;
    let s3 = s_ * s_ * s_;

    // LMS -> linear sRGB
    let r_lin = 4.0767416621 * l3 - 3.3077115913 * m3 + 0.2309699292 * s3;
    let g_lin = -1.2684380046 * l3 + 2.6097574011 * m3 - 0.3413193965 * s3;
    let b_lin = -0.0041960863 * l3 - 0.7034186147 * m3 + 1.7076147010 * s3;

    (r_lin, g_lin, b_lin)
}

fn srgb_encode(c: f64) -> f64 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn clamp01(c: f64) -> f64 {
    c.clamp(0.0, 1.0)
}

/// WCAG 2.x relative luminance of a straight sRGB channel triplet.
fn wcag_relative_luminance(color: Rgba) -> f64 {
    fn linearize(c: f32) -> f64 {
        let c = c as f64;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * linearize(color.r) + 0.7152 * linearize(color.g) + 0.0722 * linearize(color.b)
}

/// WCAG 2.x contrast ratio between two opaque sRGB colours (alpha ignored —
/// every pair the AA gate checks is fully opaque). Always >= 1.0.
pub fn contrast_ratio(a: Rgba, b: Rgba) -> f64 {
    let la = wcag_relative_luminance(a);
    let lb = wcag_relative_luminance(b);
    let (lighter, darker) = if la >= lb { (la, lb) } else { (lb, la) };
    (lighter + 0.05) / (darker + 0.05)
}
