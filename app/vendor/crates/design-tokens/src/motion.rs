//! The register motion set (plan 094 revision 3, lane K6, spec R17 and R18) in iced 0.14:
//! the tier 1 forms. An app keeps these values in its state, calls them in `view` with the
//! current `Instant`, and asks for a redraw while [`Ink::is_running`] or
//! `Animation::is_animating` is true. Every time comes from `scale::MOTION`.
//!
//! Tier 1 here: F1 receipt swap ([`Receipt`]), F2 entry ink ([`Ink`], [`ink_color`]) with
//! the many-changes cap ([`InkCap`]), D1 value change (ink plus a delta text), D2 sort mark
//! ([`sort_turn`]), D3 gauge tick travel and P1 determinate fill ([`travel`]), D4 series
//! extend ([`draw_in`]), S2 still first paint (nothing to animate). Tier 2 (no iced API):
//! the F1 opacity swap, F3 refusal ring, N1 sheet change, N2 key carry and S1 theme
//! cross-fade; their rows and citations are in `vault/knowledge/wiki/design-language/motion.md`.
use crate::scale::MOTION;
use iced_core::animation::{Animation, Easing};
use iced_core::time::{Duration, Instant};
use iced_core::Color;
use std::collections::VecDeque;
use std::sync::OnceLock;

fn ms(value: f32) -> Duration {
    Duration::from_secs_f32(value / 1000.0)
}

/// The four numbers of `MOTION.ease_standard` (`cubic-bezier(x1,y1,x2,y2)`).
fn standard_points() -> [f32; 4] {
    static POINTS: OnceLock<[f32; 4]> = OnceLock::new();
    *POINTS.get_or_init(|| {
        let inner = MOTION
            .ease_standard
            .trim_start_matches("cubic-bezier(")
            .trim_end_matches(')');
        let mut out = [0.0; 4];
        for (slot, part) in out.iter_mut().zip(inner.split(',')) {
            *slot = part
                .trim()
                .parse()
                .expect("ease_standard is a cubic-bezier of four numbers");
        }
        out
    })
}

fn bezier(t: f32, p1: f32, p2: f32) -> f32 {
    let u = 1.0 - t;
    3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t
}

/// `ease_standard` as an easing function: the CSS cubic-bezier solved for x by bisection.
pub fn ease_standard(x: f32) -> f32 {
    let [x1, y1, x2, y2] = standard_points();
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..32 {
        let mid = (lo + hi) / 2.0;
        if bezier(mid, x1, x2) < x {
            lo = mid
        } else {
            hi = mid
        }
    }
    bezier((lo + hi) / 2.0, y1, y2)
}

/// `Easing::Custom` over [`ease_standard`], for any `Animation` of the set.
pub const EASE_STANDARD: Easing = Easing::Custom(ease_standard);

/// F2 entry ink on one cell: full at once, held for `ink_hold`, then back over `slow`.
/// With reduced motion the fade is instant and the hold stays the same.
#[derive(Debug, Clone)]
pub struct Ink {
    dries_at: Instant,
    fade: Animation<bool>,
}

impl Ink {
    pub fn new(at: Instant, reduced: bool) -> Self {
        let dries_at = at + ms(MOTION.ink_hold);
        let fade_ms = if reduced { 0.0 } else { MOTION.slow };
        let fade = Animation::new(true)
            .easing(EASE_STANDARD)
            .duration(ms(fade_ms))
            .go(false, dries_at);
        Self { dries_at, fade }
    }
    /// 1.0 = the accent soft tone, 0.0 = the row tone.
    pub fn level(&self, at: Instant) -> f32 {
        if at < self.dries_at {
            1.0
        } else {
            self.fade.interpolate(0.0, 1.0, at)
        }
    }
    pub fn is_running(&self, at: Instant) -> bool {
        at < self.dries_at || self.fade.is_animating(at)
    }
}

/// The colour of an inked cell at `level` (see [`Ink::level`]): a straight mix of the row
/// tone and the theme's `accent_soft`. A fill, never an edge bar or a glow.
pub fn ink_color(row: Color, soft: Color, level: f32) -> Color {
    let k = level.clamp(0.0, 1.0);
    let mix = |a: f32, b: f32| a + (b - a) * k;
    Color {
        r: mix(row.r, soft.r),
        g: mix(row.g, soft.g),
        b: mix(row.b, soft.b),
        a: mix(row.a, soft.a),
    }
}

/// The many-changes cap of one view: above `ink_cap_per_second` changes in one second, no
/// cell gets ink; the app clears the ink that is on and writes what changed in the footer.
#[derive(Debug, Clone, Default)]
pub struct InkCap {
    changes: VecDeque<Instant>,
}

impl InkCap {
    /// Notes one change. `true`: ink it. `false`: the view is over the cap.
    pub fn admit(&mut self, at: Instant) -> bool {
        while self
            .changes
            .front()
            .is_some_and(|t| at.duration_since(*t) >= Duration::from_secs(1))
        {
            self.changes.pop_front();
        }
        self.changes.push_back(at);
        self.changes.len() <= MOTION.ink_cap_per_second as usize
    }
    /// The changes in the last second (for the footer text).
    pub fn count(&self) -> usize {
        self.changes.len()
    }
}

/// F1: whether the button shows the past-tense result ("Saved") at `at`. The swap is tier 1;
/// its opacity fade is tier 2 (a button style has no opacity of its own content).
#[derive(Debug, Clone, Copy)]
pub struct Receipt {
    until: Instant,
}

impl Receipt {
    pub fn new(at: Instant) -> Self {
        Self {
            until: at + ms(MOTION.ink_hold),
        }
    }
    pub fn shows_result(&self, at: Instant) -> bool {
        at < self.until
    }
}

/// D3 gauge tick and P1 fill: a value that travels from where it was over `base`. Call
/// `go_mut(new, now)`; read `interpolate_with(|v| v, now)`. Reduced motion: it jumps.
pub fn travel(start: f32, reduced: bool) -> Animation<f32> {
    let duration = if reduced { 0.0 } else { MOTION.base };
    Animation::new(start)
        .easing(EASE_STANDARD)
        .duration(ms(duration))
}

/// D4: the drawn share (0 to 1) of a new series segment, from `at`, over `base`.
pub fn draw_in(at: Instant, reduced: bool) -> Animation<bool> {
    let duration = if reduced { 0.0 } else { MOTION.base };
    Animation::new(false)
        .easing(EASE_STANDARD)
        .duration(ms(duration))
        .go(true, at)
}

/// D2: the sort mark angle in radians, 0 ascending and pi (half a turn) descending.
pub fn sort_turn(descending: bool, at: Instant, reduced: bool) -> Animation<bool> {
    let duration = if reduced { 0.0 } else { MOTION.base };
    Animation::new(!descending)
        .easing(EASE_STANDARD)
        .duration(ms(duration))
        .go(descending, at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn after(t0: Instant, value: f32) -> Instant {
        t0 + ms(value)
    }

    #[test]
    fn ease_standard_is_the_token_curve_an_ease_out_without_overshoot() {
        assert_eq!(standard_points(), [0.2, 0.0, 0.0, 1.0]);
        assert_eq!(ease_standard(0.0), 0.0);
        assert_eq!(ease_standard(1.0), 1.0);
        let mut last = 0.0;
        for i in 1..=100 {
            let y = ease_standard(i as f32 / 100.0);
            assert!(
                y >= last && y <= 1.0 + 1e-5,
                "not monotone or above 1 at {i}: {y}"
            );
            last = y;
        }
        assert!(
            ease_standard(0.5) > 0.8,
            "an ease-out is past 80 % at half time"
        );
    }

    #[test]
    fn ink_holds_for_ink_hold_then_dries_over_slow() {
        let t0 = Instant::now();
        let ink = Ink::new(t0, false);
        assert_eq!(ink.level(t0), 1.0);
        assert_eq!(ink.level(after(t0, MOTION.ink_hold - 1.0)), 1.0);
        let mid = ink.level(after(t0, MOTION.ink_hold + MOTION.slow / 2.0));
        assert!(mid > 0.0 && mid < 1.0, "half way through the fade: {mid}");
        assert_eq!(
            ink.level(after(t0, MOTION.ink_hold + MOTION.slow + 1.0)),
            0.0
        );
        assert!(ink.is_running(after(t0, MOTION.ink_hold + 10.0)));
        assert!(!ink.is_running(after(t0, MOTION.ink_hold + MOTION.slow + 1.0)));
    }

    #[test]
    fn reduced_ink_keeps_the_hold_and_goes_off_at_once() {
        let t0 = Instant::now();
        let ink = Ink::new(t0, true);
        assert_eq!(ink.level(after(t0, MOTION.ink_hold - 1.0)), 1.0);
        assert_eq!(ink.level(after(t0, MOTION.ink_hold + 1.0)), 0.0);
    }

    #[test]
    fn ink_color_mixes_the_row_tone_and_accent_soft() {
        let row = Color {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 1.0,
        };
        let soft = Color {
            r: 0.0,
            g: 0.5,
            b: 1.0,
            a: 1.0,
        };
        assert_eq!(ink_color(row, soft, 1.0), soft);
        assert_eq!(ink_color(row, soft, 0.0), row);
        assert_eq!(
            ink_color(row, soft, 0.5),
            Color {
                r: 0.5,
                g: 0.75,
                b: 1.0,
                a: 1.0
            }
        );
    }

    #[test]
    fn the_cap_admits_six_changes_per_second_and_refuses_the_seventh() {
        let t0 = Instant::now();
        let mut cap = InkCap::default();
        let admitted: Vec<bool> = (0..7)
            .map(|i| cap.admit(after(t0, i as f32 * 10.0)))
            .collect();
        assert_eq!(admitted, vec![true, true, true, true, true, true, false]);
        assert_eq!(cap.count(), 7);
        assert!(
            cap.admit(after(t0, 1100.0)),
            "a second later the view inks again"
        );
        assert_eq!(cap.count(), 1);
    }

    #[test]
    fn the_receipt_shows_the_result_for_ink_hold() {
        let t0 = Instant::now();
        let r = Receipt::new(t0);
        assert!(r.shows_result(after(t0, MOTION.ink_hold - 1.0)));
        assert!(!r.shows_result(after(t0, MOTION.ink_hold)));
    }

    #[test]
    fn travel_moves_from_the_old_value_to_the_new_one_over_base_without_overshoot() {
        let t0 = Instant::now();
        let tick = travel(0.42, false).go(0.67, t0);
        assert!((tick.interpolate_with(|v| v, t0) - 0.42).abs() < 1e-6);
        let mid = tick.interpolate_with(|v| v, after(t0, MOTION.base / 2.0));
        assert!(mid > 0.42 && mid < 0.67, "between the values: {mid}");
        assert!((tick.interpolate_with(|v| v, after(t0, MOTION.base + 1.0)) - 0.67).abs() < 1e-6);
        let jump = travel(0.42, true).go(0.67, t0);
        assert!(
            (jump.interpolate_with(|v| v, t0) - 0.67).abs() < 1e-6,
            "reduced motion: the tick jumps"
        );
    }

    #[test]
    fn a_new_segment_draws_in_over_base_and_the_sort_mark_turns_half_a_turn() {
        let t0 = Instant::now();
        let seg = draw_in(t0, false);
        assert_eq!(seg.interpolate(0.0, 1.0, t0), 0.0);
        assert_eq!(seg.interpolate(0.0, 1.0, after(t0, MOTION.base + 1.0)), 1.0);
        assert_eq!(draw_in(t0, true).interpolate(0.0, 1.0, t0), 1.0);
        let mark = sort_turn(true, t0, false);
        assert_eq!(mark.interpolate(0.0, PI, t0), 0.0);
        assert_eq!(mark.interpolate(0.0, PI, after(t0, MOTION.base + 1.0)), PI);
    }
}
