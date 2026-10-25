use alidade_app::chart::key_entries;
use alidade_app::screens::drawing::{scale_texts, Scale};
use alidade_app::theme::Palette;

#[test]
fn continuous_keys_cover_every_series_with_its_exact_stroke() {
    // R2-F1/M1: each panel's rendered key keeps the six specified labels and strokes.
    let palette = Palette::light();
    let keys = [
        key_entries(palette, 0),
        key_entries(palette, 1),
        key_entries(palette, 2),
    ];
    assert_eq!(
        keys.each_ref()
            .map(|entries| entries.iter().map(|(label, _)| *label).collect::<Vec<_>>()),
        [
            vec!["Download", "Upload"],
            vec!["Idle", "Load", "Jitter"],
            vec!["Packet loss"]
        ]
    );
    let strokes: Vec<_> = keys
        .into_iter()
        .flatten()
        .map(|(_, stroke)| stroke)
        .collect();
    assert_eq!(
        strokes.iter().map(|s| s.width).collect::<Vec<_>>(),
        [2.0; 6]
    );
    assert_eq!(
        strokes
            .iter()
            .map(|s| s.line_dash.segments)
            .collect::<Vec<_>>(),
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
        strokes.iter().map(|s| s.style).collect::<Vec<_>>(),
        Palette::light()
            .theme
            .chart
            .map(|c| iced::widget::canvas::Style::Solid(c.into()))
    );
}

#[test]
fn latency_scale_end_matches_summary_precision() {
    // R2-F1/M3: the canvas text for 42.8858 ms rounds to 42.9 ms, like the summary.
    assert_eq!(
        scale_texts(
            &Scale {
                unit: "ms",
                values: [Some(13.7), Some(42.8858)]
            },
            0.0,
            400.0,
            ["Idle ping", "Under load"],
            Palette::light()
        )[4]
        .content,
        "42.9 ms"
    );
    assert_eq!(
        scale_texts(
            &Scale {
                unit: "Mbit/s",
                values: [Some(94.03164), Some(27.89626)]
            },
            0.0,
            400.0,
            ["Download", "Upload"],
            Palette::light()
        )[4]
        .content,
        "94.03 Mbit/s"
    );
}
