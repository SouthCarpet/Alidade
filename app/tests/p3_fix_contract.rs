use alidade_app::chart::series_specs;
use alidade_app::screens::drawing::Scale;
use alidade_app::screens::RoundColumn;
use alidade_app::theme::Palette;
use iced::alignment::Horizontal;

#[test]
fn continuous_keys_cover_every_series_with_its_exact_stroke() {
    let specs = series_specs(Palette::light());
    assert_eq!(
        specs.map(|s| (s.panel, s.label)),
        [
            (0, "Download"),
            (0, "Upload"),
            (1, "Idle"),
            (1, "Load"),
            (1, "Jitter"),
            (2, "Packet loss"),
        ]
    );
    assert_eq!(specs.map(|s| s.stroke.width), [2.0; 6]);
    assert_eq!(
        specs.map(|s| s.stroke.line_dash.segments),
        [
            &[][..],
            &[8.0, 5.0],
            &[1.5, 4.0],
            &[8.0, 4.0, 1.5, 4.0],
            &[16.0, 6.0],
            &[8.0, 4.0, 1.5, 4.0, 1.5, 4.0],
        ]
    );
    assert_eq!(
        specs.map(|s| s.stroke.style),
        Palette::light()
            .theme
            .chart
            .map(|c| iced::widget::canvas::Style::Solid(c.into()))
    );
}

#[test]
fn rounds_numeric_columns_align_headers_and_values_right() {
    assert_eq!(
        [
            RoundColumn::Down.alignment(),
            RoundColumn::Up.alignment(),
            RoundColumn::PingIdle.alignment(),
            RoundColumn::PingLoad.alignment(),
            RoundColumn::Jitter.alignment(),
            RoundColumn::Loss.alignment(),
        ],
        [Horizontal::Right; 6]
    );
    assert_eq!(RoundColumn::Time.alignment(), Horizontal::Left);
}

#[test]
fn latency_scale_end_matches_summary_precision() {
    assert_eq!(
        Scale {
            unit: "ms",
            values: [Some(13.7), Some(42.8858)]
        }
        .end_label(),
        "42.9 ms"
    );
    assert_eq!(
        Scale {
            unit: "Mbit/s",
            values: [Some(94.03164), Some(27.89626)]
        }
        .end_label(),
        "94.03 Mbit/s"
    );
}
