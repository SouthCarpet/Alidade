use std::time::{Duration, Instant, UNIX_EPOCH};

use alidade_app::screens::drawing::{diagram, Diagram, Primitive};
use alidade_app::screens::settings::unverified_status;
use alidade_app::state::{LatestRound, MeasurementDrawIn, RoundOutcome};
use alidade_app::theme::Palette;
use alidade_engine::{PingStats, RoundKind, RoundResult, Throughput};

fn round() -> LatestRound {
    LatestRound::Live(Box::new(RoundOutcome {
        result: RoundResult {
            started_at: UNIX_EPOCH,
            kind: RoundKind::Full,
            down: Some(Throughput {
                bits_per_sec: 123_450_000.0,
                bytes: 10,
                duration: Duration::from_secs(1),
                capped: false,
            }),
            up: Some(Throughput {
                bits_per_sec: 21_500_000.0,
                bytes: 10,
                duration: Duration::from_secs(1),
                capped: false,
            }),
            ping_idle: Some(ping(12.0)),
            ping_down: Some(ping(20.0)),
            ping_up: Some(ping(40.0)),
            down_load: None,
            up_load: None,
            capped: false,
            skipped_reason: None,
        },
        persist_error: None,
    }))
}

fn ping(ms: f64) -> PingStats {
    PingStats {
        avg_ms: Some(ms),
        min_ms: Some(ms),
        max_ms: Some(ms),
        jitter_ms: None,
        loss_pct: 0.0,
        sent: 5,
    }
}

#[test]
fn latest_round_supplies_bars_marks_and_ping_difference_without_fake_values() {
    // Brief T3.2: 123.45/21.5 Mbit/s, idle 12 ms, mean load 30 ms, delta +18 ms.
    let data = Diagram::from_round(&round());
    assert_eq!(data.speed.values, [Some(123.45), Some(21.5)]);
    assert_eq!(data.latency.values, [Some(12.0), Some(30.0)]);
    assert_eq!(data.difference_text(), "Ping change under load: +18.0 ms");
}

#[test]
fn missing_round_has_a_reason_and_no_diagram() {
    // Brief T3.2: no round means no geometry and one plain reason.
    assert_eq!(
        diagram(None).unwrap_err(),
        "No diagram: no round has been measured."
    );
}

#[test]
fn throughput_and_latency_use_independent_zero_based_domains_and_units() {
    // Brief T3.2: each domain ends at its own largest real value, never the other unit.
    let data = Diagram::from_round(&round());
    assert_eq!(data.speed.domain(), (0.0, 123.45));
    assert_eq!(data.latency.domain(), (0.0, 30.0));
    assert_eq!((data.speed.unit, data.latency.unit), ("Mbit/s", "ms"));
}

#[test]
fn draw_list_contains_only_two_speed_bars_and_two_latency_marks() {
    // R34(e): the renderer consumes this complete list, which has no dimension primitive.
    assert_eq!(
        Diagram::from_round(&round()).primitives(),
        vec![
            Primitive::Bar {
                series: 0,
                fraction: 1.0
            },
            Primitive::Bar {
                series: 1,
                fraction: 21.5 / 123.45
            },
            Primitive::Mark {
                series: 0,
                fraction: 0.4
            },
            Primitive::Mark {
                series: 1,
                fraction: 1.0
            },
        ]
    );
}

#[test]
fn missing_measurements_keep_gaps_and_explain_the_missing_difference() {
    // Brief real-values-only rule: absent phases are not zero bars or zero ping.
    let LatestRound::Live(mut outcome) = round() else {
        unreachable!()
    };
    outcome.result.down = None;
    outcome.result.ping_down = None;
    outcome.result.ping_up = None;
    let data = Diagram::from_round(&LatestRound::Live(outcome));
    assert_eq!(data.speed.values, [None, Some(21.5)]);
    assert_eq!(data.latency.values, [Some(12.0), None]);
    assert_eq!(
        data.difference_text(),
        "Ping change unavailable: both ping values are needed."
    );
}

#[test]
fn reduced_motion_shows_all_three_final_moves_at_once() {
    // R32: at the same injected start instant, every mark is final and no frame is requested.
    let at = Instant::now();
    let motion = MeasurementDrawIn::new(at, true);
    assert_eq!(motion.frames(at), [(0.0, 1.0); 3]);
    assert!(!motion.is_animating(at));
}

#[test]
fn new_measurement_uses_three_moves_fifty_ms_apart_and_finishes_by_320_ms() {
    // R32: 0/50/100 ms starts, 120 ms each, 220 ms total; position and alpha only.
    let at = Instant::now();
    let motion = MeasurementDrawIn::new(at, false);
    assert_eq!(
        motion.delays(),
        [
            Duration::ZERO,
            Duration::from_millis(50),
            Duration::from_millis(100)
        ]
    );
    assert_eq!(motion.frames(at), [(-8.0, 0.0); 3]);
    assert_eq!(
        motion.frames(at + Duration::from_millis(220)),
        [(0.0, 1.0); 3]
    );
    assert!(!motion.is_animating(at + Duration::from_millis(320)));
    assert!(motion.duration() <= Duration::from_millis(320));
}

#[test]
fn unverified_target_stays_plain_text_with_aa_contrast_in_both_themes() {
    // T3.1 COLOR-05: preserve the state meaning and meet 4.5:1 on the target well.
    let (label, light) = unverified_status(Palette::light());
    let (dark_label, dark) = unverified_status(Palette::dark());
    assert_eq!((label, dark_label), ("unverified", "unverified"));
    assert!(design_tokens::contrast_ratio(light, Palette::light().theme.well) >= 4.5);
    assert!(design_tokens::contrast_ratio(dark, Palette::dark().theme.well) >= 4.5);
}

#[test]
fn continuous_chart_never_maps_loss_percent_onto_the_latency_scale() {
    // R34(e): 100% loss must not expand the 12 ms latency domain; separate units and plots.
    use alidade_app::chart::{chart_domains, panel_bounds, Point};
    let domains = chart_domains(&[Point {
        id: 1,
        at: UNIX_EPOCH,
        download_mbit: Some(123.45),
        upload_mbit: Some(21.5),
        ping_idle_ms: Some(12.0),
        ping_load_ms: None,
        jitter_ms: None,
        loss_pct: Some(100.0),
    }]);
    assert_eq!(domains[1], (0.0, 13.0));
    assert_eq!(domains[2], (0.0, 105.0));
    let [speed, latency, loss] = panel_bounds(iced::Size::new(900.0, 240.0));
    assert!(speed.x + speed.width < latency.x);
    assert!(latency.x + latency.width < loss.x);
}
