//! Fira Sans for iced (plan 125, spec R23). Load every file in [`FILES`] at
//! start (`iced::application(..).font(bytes)` once per entry), then set
//! [`regular`] as the default font.
//!
//! Numbers: iced 0.14's `Font` has no OpenType feature switch, so a text run
//! cannot ask for `tnum`. A number run uses [`numeric`]: `Fira Sans Tabular`,
//! a renamed copy of Fira Sans whose character map points the digits, currency
//! and maths signs to their own tabular glyphs (plain zero, two contours). It is
//! built by `design/scripts/build_fira_tabular.py` and checked by
//! `test_fira_tabular.py`. Never `Font::MONOSPACE`: the system monospace face
//! can draw a slashed or dotted zero.

use iced_core::font::{Family, Weight};
use iced_core::Font;

/// The text family, the same face as web `--font-sans`.
pub const FAMILY: &str = "Fira Sans";
/// The number family: Fira Sans with its tabular figures in the character map.
pub const TABULAR_FAMILY: &str = "Fira Sans Tabular";

macro_rules! font_file {
    ($name:literal) => {
        include_bytes!(concat!("../../../assets/fonts/fira-sans/", $name))
    };
}

/// Every vendored file, text faces first: 400, 500, 600 and 700 of each family.
pub static FILES: [&[u8]; 8] = [
    font_file!("FiraSans-Regular.ttf"),
    font_file!("FiraSans-Medium.ttf"),
    font_file!("FiraSans-SemiBold.ttf"),
    font_file!("FiraSans-Bold.ttf"),
    font_file!("FiraSansTabular-Regular.ttf"),
    font_file!("FiraSansTabular-Medium.ttf"),
    font_file!("FiraSansTabular-SemiBold.ttf"),
    font_file!("FiraSansTabular-Bold.ttf"),
];

const fn face(family: &'static str, weight: Weight) -> Font {
    Font {
        family: Family::Name(family),
        weight,
        ..Font::DEFAULT
    }
}

/// Body text, 400. The default font.
pub const fn regular() -> Font {
    face(FAMILY, Weight::Normal)
}
/// 500.
pub const fn medium() -> Font {
    face(FAMILY, Weight::Medium)
}
/// 600: labels and section headings.
pub const fn semibold() -> Font {
    face(FAMILY, Weight::Semibold)
}
/// 700: titles, display steps and totals.
pub const fn bold() -> Font {
    face(FAMILY, Weight::Bold)
}
/// A number run at `weight`, with tabular figures.
pub const fn numeric(weight: Weight) -> Font {
    face(TABULAR_FAMILY, weight)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_is_a_truetype_font() {
        for bytes in FILES {
            assert_eq!(
                &bytes[..4],
                &[0, 1, 0, 0],
                "a TrueType file starts with version 1.0"
            );
        }
    }

    #[test]
    fn text_and_numbers_use_their_own_families() {
        assert_eq!(regular().family, Family::Name("Fira Sans"));
        assert_eq!(
            numeric(Weight::Bold),
            Font {
                family: Family::Name("Fira Sans Tabular"),
                weight: Weight::Bold,
                ..Font::DEFAULT
            }
        );
    }
}
