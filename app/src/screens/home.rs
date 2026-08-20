//! Home screen (plan Task 2): five KPI tiles, "Run single test", and the
//! empty/error states around it.
//!
//! The core rule, straight from the brief: a missing measurement is shown
//! as missing, with its reason — never as `0.00`, never as a bare dash that
//! could mean either. `RoundResult`'s `down`/`up`/`ping_*` fields are all
//! `Option` for exactly that reason (see `engine/src/round.rs`), and every
//! tile below reads that `Option` honestly rather than defaulting it away.

use design_tokens::scale::{RADIUS, SPACE, TYPE};
use iced::alignment::Vertical;
use iced::widget::{button, column, container, row, text, Space, Stack};
use iced::{Border, Center, Color, Element, Fill, Length};

use alidade_engine::{LoadWindow, PingStats, RoundResult, Throughput};

use crate::state::{App, Message, Screen};
use crate::theme::Palette;
use crate::ui::{font_medium, font_numeric, font_regular};

pub fn view(app: &App) -> Element<'_, Message> {
    let palette = Palette::light();

    let mut sections: Vec<Element<'_, Message>> = Vec::new();

    if let Some(message) = &app.settings_error {
        sections.push(banner(palette, message));
    }

    sections.push(actions(palette, app));

    match &app.last_round {
        None => sections.push(empty_state(palette)),
        Some(outcome) => {
            if let Some(err) = &outcome.persist_error {
                sections.push(banner(
                    palette,
                    &format!("round measured but could not be saved: {err}"),
                ));
            }
            if outcome.result.capped {
                sections.push(banner(
                    palette,
                    "capped — the daily data budget ended a throughput phase early",
                ));
            }
            sections.push(tiles(palette, &outcome.result));
        }
    }

    container(column(sections).spacing(SPACE.s6).width(Fill))
        .width(Fill)
        .height(Fill)
        .into()
}

// ---------------------------------------------------------------------
// Actions: Run single test / Start continuous, and the "Measuring" pill
// ---------------------------------------------------------------------

fn actions(palette: Palette, app: &App) -> Element<'_, Message> {
    let run_button = button(text("Run single test").size(TYPE.sm).font(font_medium()))
        .padding([SPACE.s2, SPACE.s4])
        .style(palette.btn_primary())
        .on_press_maybe((!app.round_running).then_some(Message::RunSingleTest));

    // Continuous scheduling is Task 4/6's scope (it needs the engine
    // `Scheduler`, not wired into `App` yet); this button takes the user to
    // that screen rather than faking a mode that does not run anything.
    let continuous_button = button(text("Start continuous").size(TYPE.sm).font(font_medium()))
        .padding([SPACE.s2, SPACE.s4])
        .style(palette.btn_secondary())
        .on_press(Message::ScreenSelected(Screen::Continuous));

    let mut bar = row![run_button, continuous_button].spacing(SPACE.s3).align_y(Center);
    if app.round_running {
        bar = bar.push(status_pill(palette, "Measuring"));
    }
    bar.into()
}

fn status_pill<'a>(palette: Palette, label: &'static str) -> Element<'a, Message> {
    let background = Color::from(palette.theme.accent_soft);
    let foreground = Color::from(palette.theme.accent_soft_foreground);
    container(text(label).size(TYPE.xs).font(font_medium()).color(foreground))
        .padding([SPACE.s1, SPACE.s3])
        .style(move |_| {
            container::Style::default().background(background).border(Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: iced::border::radius(RADIUS.pill),
            })
        })
        .into()
}

/// A recessed well: `well` background + `border`, no inset shadow — the
/// same recipe as `design/gallery-iced/src/main.rs`'s `well()`, scoped
/// locally here since Home is its only caller so far.
fn well<'a>(palette: Palette, content: Element<'a, Message>) -> Element<'a, Message> {
    let border = Color::from(palette.theme.border);
    container(content)
        .padding(SPACE.s3)
        .width(Fill)
        .style(move |_| {
            container::Style::default()
                .background(palette.well())
                .border(Border { color: border, width: 1.0, radius: iced::border::radius(RADIUS.md) })
        })
        .into()
}

fn banner<'a>(palette: Palette, message: &str) -> Element<'a, Message> {
    let dot_color = Color::from(palette.theme.danger);
    let dot = container(Space::new().width(Length::Fixed(8.0)).height(Length::Fixed(8.0))).style(move |_| {
        container::Style::default().background(dot_color).border(Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: iced::border::radius(RADIUS.pill),
        })
    });

    well(
        palette,
        row![dot, text(message.to_string()).size(TYPE.sm).color(palette.text())]
            .spacing(SPACE.s2)
            .align_y(Center)
            .into(),
    )
}

fn empty_state<'a>(palette: Palette) -> Element<'a, Message> {
    let icon_color = Color::from(palette.theme.border);
    let icon = container(Space::new().width(Length::Fixed(26.0)).height(Length::Fixed(26.0))).style(move |_| {
        container::Style::default().background(icon_color).border(Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: iced::border::radius(RADIUS.pill),
        })
    });

    let body = column![
        icon,
        text("No rounds yet — run a single test or start continuous mode.")
            .size(TYPE.sm)
            .font(font_medium())
            .color(palette.text()),
    ]
    .spacing(SPACE.s3)
    .align_x(Center)
    .width(Fill);

    well(palette, container(body).width(Fill).padding(SPACE.s6).into())
}

// ---------------------------------------------------------------------
// KPI tiles
// ---------------------------------------------------------------------

fn tiles<'a>(palette: Palette, result: &RoundResult) -> Element<'a, Message> {
    let download = throughput_value(result.down, "download", result.skipped_reason.as_deref());
    let upload = throughput_value(result.up, "upload", result.skipped_reason.as_deref());
    let idle = ping_idle_value(result.ping_idle, result.skipped_reason.as_deref());
    let under_load =
        ping_under_load_value(result.ping_down, result.ping_up, result.down_load, result.up_load);
    let loss = loss_value(result.ping_idle, result.skipped_reason.as_deref());

    // `meta` (min/max/current, for the gauge's 2px tick) is `None` at every
    // call site: Task 2 never has round history yet, only this one just-
    // measured round. Passing `None` here is the honest "lone value" case
    // `rule_design_language.md` point 3(b) names — Task 4/5's aggregates are
    // this signature's real consumer.
    let items = vec![
        kpi_tile(palette, "Download", &download, "Mbit/s", None),
        kpi_tile(palette, "Upload", &upload, "Mbit/s", None),
        kpi_tile(palette, "Idle ping", &idle, "ms", None),
        kpi_tile(palette, "Ping under load", &under_load, "ms", None),
        kpi_tile(palette, "Loss", &loss, "%", None),
    ];
    row(items).spacing(SPACE.s4).into()
}

/// One KPI tile: label / value (or the reason there is none) / micro-gauge.
/// `meta` is `(min, max, current)` for the 2px tick; `None` draws the 3px
/// rule as a baseline only (`rule_design_language.md` point 3(b): "a lone
/// value ... uses the 3px rule as a baseline only, no fake span").
fn kpi_tile<'a>(
    palette: Palette,
    label: &'static str,
    value: &TileValue,
    unit: &'static str,
    meta: Option<(f64, f64, f64)>,
) -> Element<'a, Message> {
    let label_text = text(tracked(label)).size(TYPE.xs).font(font_medium()).color(palette.muted());

    let value_row: Element<'a, Message> = match value {
        TileValue::Value(digits) => {
            let numeral_font = iced::Font { weight: iced::font::Weight::Semibold, ..font_numeric() };
            row![
                text(digits.clone()).size(TYPE.xxl).font(numeral_font).color(palette.text()),
                text(format!(" {unit}")).size(TYPE.sm).font(font_medium()).color(palette.muted()),
            ]
            .align_y(Vertical::Bottom)
            .into()
        }
        TileValue::Missing(reason) => {
            text(reason.clone()).size(TYPE.sm).font(font_regular()).color(palette.muted()).into()
        }
    };

    let gauge = micro_gauge(palette, meta.map(|(min, max, current)| gauge_fraction(min, max, current)));

    palette.card(true, column![label_text, value_row, gauge].spacing(SPACE.s2))
}

const GAUGE_H: f32 = 9.0;
const GAUGE_BAR_H: f32 = 3.0;
const GAUGE_TICK_W: f32 = 2.0;
const GAUGE_TICK_H: f32 = 7.0;

/// The Tier-1 KPI micro-gauge (`rule_design_language.md` point 3(b)): a 3px
/// tabular baseline at 40% `border`, with a 2px `accent_active` tick at the
/// current value when `tick_frac` is `Some`. Positions the tick with two
/// `FillPortion` spacers rather than a fixed pixel width (unlike the
/// gallery's demo tile), so it stays correct at whatever width the tile
/// actually renders at.
fn micro_gauge<'a>(palette: Palette, tick_frac: Option<f32>) -> Element<'a, Message> {
    let baseline_color = Color { a: 0.4, ..Color::from(palette.theme.border) };
    let baseline_bar = container(Space::new().width(Fill).height(Length::Fixed(GAUGE_BAR_H))).style(move |_| {
        container::Style::default().background(baseline_color).border(Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: iced::border::radius(RADIUS.pill),
        })
    });
    let baseline_layer: Element<'a, Message> =
        container(baseline_bar).width(Fill).height(Length::Fixed(GAUGE_H)).align_y(Vertical::Center).into();

    let Some(fraction) = tick_frac else {
        return baseline_layer;
    };

    let fraction = fraction.clamp(0.0, 1.0);
    let before = ((fraction * 1000.0).round() as u16).max(1);
    let after = (((1.0 - fraction) * 1000.0).round() as u16).max(1);
    let tick_color = Color::from(palette.theme.accent_active);
    let tick_mark = container(Space::new().width(Length::Fixed(GAUGE_TICK_W)).height(Length::Fixed(GAUGE_TICK_H)))
        .style(move |_| container::Style::default().background(tick_color));
    let tick_layer: Element<'a, Message> = row![
        Space::new().width(Length::FillPortion(before)),
        tick_mark,
        Space::new().width(Length::FillPortion(after)),
    ]
    .width(Fill)
    .height(Length::Fixed(GAUGE_H))
    .align_y(Vertical::Center)
    .into();

    Stack::new()
        .width(Fill)
        .height(Length::Fixed(GAUGE_H))
        .push(baseline_layer)
        .push(tick_layer)
        .into()
}

/// Best-effort +0.04em label tracking (Tier-2 fallback: `iced_core::Text`
/// has no letter-spacing field at all — see the gallery's `tracked_upper`
/// doc comment for the same gap). Unlike `tracked_upper` this keeps the
/// label's natural case ("Download", not "DOWNLOAD"): the KPI label role is
/// not the section-title role that one is for.
fn tracked(label: &str) -> String {
    let mut out = String::with_capacity(label.len() * 2);
    for (index, ch) in label.chars().enumerate() {
        if index > 0 {
            out.push('\u{2009}');
        }
        out.push(ch);
    }
    out
}

// ---------------------------------------------------------------------
// Pure helpers: `Option<f64>` (and friends) -> what the tile shows.
// The whole point of the release lives here — unit-tested below.
// ---------------------------------------------------------------------

/// What one KPI tile shows: a formatted number, or the reason there is
/// none. Mirrors the CLI's own `format_speed`/`format_ping`/
/// `format_under_load` (`cli/src/main.rs`) so the same round reads the same
/// way in both faces.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TileValue {
    Value(String),
    Missing(String),
}

/// The segment of a round's combined `skipped_reason` that names `label`
/// (`skip_reasons.join("; ")` in `engine/src/round.rs` always prefixes a
/// phase's own entry with `"<label>: "`), or a generic fallback when there
/// is none — which is the honest answer for a metric that was simply not
/// selected in settings, since `run_round` records no skip reason at all
/// for that case.
fn missing_reason(skipped_reason: Option<&str>, label: &str) -> String {
    let prefix = format!("{label}:");
    skipped_reason
        .and_then(|reasons| reasons.split("; ").find(|segment| segment.starts_with(&prefix)))
        .map(|segment| segment.trim_start_matches(&prefix).trim().to_string())
        .unwrap_or_else(|| "not selected for this round".to_string())
}

fn throughput_value(value: Option<Throughput>, label: &str, skipped_reason: Option<&str>) -> TileValue {
    match value {
        Some(t) => TileValue::Value(format!("{:.2}", t.bits_per_sec / 1_000_000.0)),
        None => TileValue::Missing(missing_reason(skipped_reason, label)),
    }
}

fn ping_idle_value(stats: Option<PingStats>, skipped_reason: Option<&str>) -> TileValue {
    match stats {
        None => TileValue::Missing(missing_reason(skipped_reason, "ping")),
        Some(stats) => match stats.avg_ms {
            Some(avg) => TileValue::Value(format!("{avg:.1}")),
            None => TileValue::Missing(format!("no answer ({:.0}% loss)", stats.loss_pct)),
        },
    }
}

/// `loss_pct` is always present on a `PingStats` that ran at all (it is the
/// measurement, not a derived average) — so `Some(0.0)` here is a real,
/// measured zero, never a stand-in for "not measured". Only the absence of
/// `PingStats` itself (ping not selected, or no target configured) is
/// `Missing`.
fn loss_value(stats: Option<PingStats>, skipped_reason: Option<&str>) -> TileValue {
    match stats {
        None => TileValue::Missing(missing_reason(skipped_reason, "ping")),
        Some(stats) => TileValue::Value(format!("{:.2}", stats.loss_pct)),
    }
}

/// One combined "ping under load" figure from the download- and
/// upload-phase samples (same combining rule as `chart.rs`'s
/// `Point::from_row`: average when both phases have a number, whichever one
/// is present when only one does).
fn ping_under_load_value(
    down_stats: Option<PingStats>,
    up_stats: Option<PingStats>,
    down_load: Option<LoadWindow>,
    up_load: Option<LoadWindow>,
) -> TileValue {
    match (down_stats.and_then(|s| s.avg_ms), up_stats.and_then(|s| s.avg_ms)) {
        (Some(down), Some(up)) => TileValue::Value(format!("{:.1}", (down + up) / 2.0)),
        (Some(value), None) | (None, Some(value)) => TileValue::Value(format!("{value:.1}")),
        (None, None) => TileValue::Missing(under_load_missing_reason(down_stats, up_stats, down_load, up_load)),
    }
}

/// The two different reasons an under-load figure can be absent, kept
/// apart (same distinction the CLI's `format_under_load` draws): a phase
/// that ran and lost every sample under load, versus one that never put
/// enough load on the link to count at all (see `MIN_UNDER_LOAD_SAMPLES`).
fn under_load_missing_reason(
    down_stats: Option<PingStats>,
    up_stats: Option<PingStats>,
    down_load: Option<LoadWindow>,
    up_load: Option<LoadWindow>,
) -> String {
    if let Some(stats) = down_stats.or(up_stats) {
        return format!("no answer ({:.0}% loss)", stats.loss_pct);
    }
    if let Some(window) = down_load.or(up_load) {
        return format!(
            "not enough load to measure ({} sample(s) in {:.1}s of load)",
            window.ping_samples,
            window.duration.as_secs_f64()
        );
    }
    "no throughput phase ran this round".to_string()
}

/// Where the micro-gauge tick sits along `[min, max]`, clamped to the track.
/// `min == max` (a single sample so far) has no span to place a tick along,
/// so it sits at the middle rather than dividing by zero.
fn gauge_fraction(min: f64, max: f64, value: f64) -> f32 {
    if max > min {
        (((value - min) / (max - min)) as f32).clamp(0.0, 1.0)
    } else {
        0.5
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn stats(avg_ms: Option<f64>, loss_pct: f64) -> PingStats {
        PingStats { avg_ms, min_ms: avg_ms, max_ms: avg_ms, jitter_ms: None, loss_pct, sent: 5 }
    }

    fn throughput(mbit: f64) -> Throughput {
        Throughput { bits_per_sec: mbit * 1_000_000.0, bytes: 0, duration: Duration::from_secs(10), capped: false }
    }

    #[test]
    fn a_measured_throughput_is_shown_in_mbit() {
        assert_eq!(
            throughput_value(Some(throughput(123.456)), "download", None),
            TileValue::Value("123.46".to_string())
        );
    }

    #[test]
    fn a_missing_throughput_shows_the_matching_skip_segment_not_a_zero() {
        let reason = "download: provider: server returned status 429; upload: partial reading discarded (1.2s of 10.0s budget measured, 12% — below the 50% needed to trust the rate)";
        assert_eq!(
            throughput_value(None, "download", Some(reason)),
            TileValue::Missing("provider: server returned status 429".to_string())
        );
        assert_eq!(
            throughput_value(None, "upload", Some(reason)),
            TileValue::Missing(
                "partial reading discarded (1.2s of 10.0s budget measured, 12% — below the 50% needed to trust the rate)"
                    .to_string()
            )
        );
    }

    #[test]
    fn a_missing_throughput_with_no_skip_reason_reads_as_not_selected() {
        assert_eq!(
            throughput_value(None, "download", None),
            TileValue::Missing("not selected for this round".to_string())
        );
    }

    #[test]
    fn ping_idle_reads_the_average_when_something_answered() {
        assert_eq!(ping_idle_value(Some(stats(Some(23.4), 0.0)), None), TileValue::Value("23.4".to_string()));
    }

    #[test]
    fn ping_idle_with_total_loss_says_so_instead_of_a_fake_average() {
        assert_eq!(
            ping_idle_value(Some(stats(None, 100.0)), None),
            TileValue::Missing("no answer (100% loss)".to_string())
        );
    }

    #[test]
    fn ping_idle_never_measured_reads_the_skip_reason() {
        assert_eq!(
            ping_idle_value(None, Some("ping: no targets configured")),
            TileValue::Missing("no targets configured".to_string())
        );
    }

    #[test]
    fn a_real_zero_loss_is_shown_as_zero_never_as_missing() {
        // The exact case this release exists to get right: a measured 0.0%
        // loss must render as a genuine number, not collapse into the same
        // "missing" bucket as a metric that was never measured at all.
        assert_eq!(loss_value(Some(stats(Some(10.0), 0.0)), None), TileValue::Value("0.00".to_string()));
    }

    #[test]
    fn loss_is_missing_only_when_ping_never_ran() {
        assert_eq!(
            loss_value(None, Some("ping: no targets configured")),
            TileValue::Missing("no targets configured".to_string())
        );
    }

    #[test]
    fn ping_under_load_averages_both_phases_when_both_answered() {
        let window = LoadWindow { duration: Duration::from_secs(10), ping_samples: 9, bytes: 1 };
        assert_eq!(
            ping_under_load_value(Some(stats(Some(20.0), 0.0)), Some(stats(Some(30.0), 0.0)), Some(window), Some(window)),
            TileValue::Value("25.0".to_string())
        );
    }

    #[test]
    fn ping_under_load_uses_whichever_phase_ran_when_only_one_did() {
        let window = LoadWindow { duration: Duration::from_secs(10), ping_samples: 9, bytes: 1 };
        assert_eq!(
            ping_under_load_value(Some(stats(Some(20.0), 0.0)), None, Some(window), None),
            TileValue::Value("20.0".to_string())
        );
    }

    #[test]
    fn ping_under_load_with_total_loss_says_so() {
        let window = LoadWindow { duration: Duration::from_secs(10), ping_samples: 9, bytes: 1 };
        assert_eq!(
            ping_under_load_value(Some(stats(None, 100.0)), None, Some(window), None),
            TileValue::Missing("no answer (100% loss)".to_string())
        );
    }

    #[test]
    fn ping_under_load_with_too_little_load_names_the_sample_count_not_a_skip() {
        let window = LoadWindow { duration: Duration::from_secs(2), ping_samples: 2, bytes: 500 };
        assert_eq!(
            ping_under_load_value(None, None, Some(window), None),
            TileValue::Missing("not enough load to measure (2 sample(s) in 2.0s of load)".to_string())
        );
    }

    #[test]
    fn ping_under_load_on_a_ping_only_round_says_no_throughput_ran() {
        assert_eq!(
            ping_under_load_value(None, None, None, None),
            TileValue::Missing("no throughput phase ran this round".to_string())
        );
    }

    #[test]
    fn gauge_fraction_normalises_into_the_track() {
        assert_eq!(gauge_fraction(0.0, 100.0, 25.0), 0.25);
        assert_eq!(gauge_fraction(0.0, 100.0, -10.0), 0.0, "below min clamps to the start of the track");
        assert_eq!(gauge_fraction(0.0, 100.0, 200.0), 1.0, "above max clamps to the end of the track");
    }

    #[test]
    fn gauge_fraction_with_no_span_sits_in_the_middle_instead_of_dividing_by_zero() {
        assert_eq!(gauge_fraction(50.0, 50.0, 50.0), 0.5);
    }
}
