//! Home: a page head, four measured facts, and an honest empty state.

use alidade_engine::{LoadWindow, PingStats, RoundKind, RoundResult, Throughput};
use design_tokens::scale::{SPACE, TYPE};
use iced::alignment::Horizontal;
use iced::widget::{button, canvas, column, container, row, text, tooltip, Canvas, Space};
use iced::{mouse, Center, Color, Element, Fill, Length, Point, Rectangle, Renderer, Theme};

use crate::state::{ActivityFrame, App, LatestRound, Message};
use crate::theme::Palette;
use crate::ui::{font_medium, font_numeric, font_regular};

/// One row of the Home summary. A missing reason never receives a unit.
#[derive(Debug, Clone, PartialEq)]
pub struct SummaryFact {
    pub label: &'static str,
    pub text: String,
    pub(crate) measured: bool,
}

/// Four facts by construction, with the same formatting for live and stored data.
pub fn summary_facts(round: &LatestRound) -> [SummaryFact; 4] {
    let [download, upload, idle, load, _loss] = round_values(round);
    [
        fact("Download", download, "Mbit/s"),
        fact("Upload", upload, "Mbit/s"),
        fact("Idle ping", idle, "ms"),
        fact("Ping under load", load, "ms"),
    ]
}

fn fact(label: &'static str, value: MetricValue, unit: &str) -> SummaryFact {
    let measured = matches!(&value, MetricValue::Value(_));
    SummaryFact {
        label,
        text: measurement_text(value, unit),
        measured,
    }
}

fn measurement_text(value: MetricValue, unit: &str) -> String {
    match value {
        MetricValue::Value(digits) => format!("{digits} {unit}"),
        MetricValue::Missing(reason) => reason,
    }
}

pub fn view(app: &App, palette: Palette) -> Element<'_, Message> {
    let activity = Canvas::new(ActivityStrip {
        palette,
        frame: app.activity.frame(app.activity_at),
    })
    .width(Fill)
    .height(2.0);
    let head = column![
        super::page_head(
            palette,
            "Link measurement",
            round_metadata(app.last_round.as_ref())
        ),
        column![actions(palette, app), activity].spacing(SPACE.s1),
    ]
    .spacing(SPACE.s3)
    .width(Fill);
    let mut sections: Vec<Element<'_, Message>> = vec![head.into()];
    if let Some(error) = &app.settings_error {
        sections.push(banner(palette, error));
    }
    if let Some(error) = &app.last_round_error {
        sections.push(banner(
            palette,
            &format!("Stored round unavailable: {error}"),
        ));
    }
    match &app.last_round {
        Some(round) => sections.extend(round_sections(
            palette,
            round,
            super::drawing::view(app, palette),
        )),
        None => {
            if app.last_round_error.is_none() {
                sections.push(empty_state(palette));
                sections.push(super::drawing::view(app, palette));
            }
        }
    }
    container(column(sections).spacing(SPACE.s4).width(Fill))
        .width(Fill)
        .height(Fill)
        .into()
}

fn round_metadata(round: Option<&LatestRound>) -> String {
    let Some(round) = round else {
        return "Local measurement history".into();
    };
    let kind = match round {
        LatestRound::Live(outcome) => outcome.result.kind,
        LatestRound::Stored(row) => row.mode,
    };
    let label = match kind {
        RoundKind::Full => "Full round",
        RoundKind::PingOnly => "Ping round",
    };
    format!("{label}, {} UTC", super::format_time(round.started_at()))
}

fn round_sections<'a>(
    palette: Palette,
    round: &LatestRound,
    diagram: Element<'a, Message>,
) -> Vec<Element<'a, Message>> {
    let mut sections = Vec::new();
    if let Some(error) = round.persist_error() {
        sections.push(banner(
            palette,
            &format!("Round measured but could not be saved: {error}"),
        ));
    }
    if round.capped() {
        sections.push(banner(
            palette,
            "The daily data budget ended a throughput phase early.",
        ));
    }
    sections.push(summary(palette, summary_facts(round)));
    sections.push(diagram);
    let [_, _, _, _, loss] = round_values(round);
    sections.push(
        text(format!("Idle packet loss: {}", measurement_text(loss, "%")))
            .size(TYPE.sm)
            .font(font_numeric())
            .color(palette.muted())
            .into(),
    );
    sections
}

fn summary<'a>(palette: Palette, facts: [SummaryFact; 4]) -> Element<'a, Message> {
    let mut rows = Vec::new();
    for (index, fact) in facts.into_iter().enumerate() {
        if index > 0 {
            rows.push(
                container(Space::new().width(Fill).height(1.0))
                    .style(move |_| container::Style::default().background(palette.border()))
                    .into(),
            );
        }
        rows.push(summary_row(palette, fact));
    }
    palette.summary_field(column(rows).width(Fill))
}

fn summary_row<'a>(palette: Palette, fact: SummaryFact) -> Element<'a, Message> {
    let shown = fact.text.split_whitespace().collect::<Vec<_>>().join(" ");
    let value = text(shown)
        .size(TYPE.md)
        .font(if fact.measured {
            font_numeric()
        } else {
            font_regular()
        })
        .color(palette.summary_ink())
        .wrapping(text::Wrapping::None)
        .align_x(if fact.measured {
            Horizontal::Right
        } else {
            Horizontal::Left
        })
        .width(Fill);
    let cells = row![
        text(fact.label)
            .size(TYPE.sm)
            .font(font_medium())
            .color(palette.summary_ink())
            .width(Length::Fixed(SPACE.s6 * 6.0)),
        container(value).width(Fill).clip(true),
    ]
    .spacing(SPACE.s3)
    .padding([SPACE.s1, 0.0])
    .align_y(Center)
    .width(Fill);
    if fact.measured {
        cells.into()
    } else {
        tooltip(
            cells,
            palette.reason_note(
                text(fact.text)
                    .size(TYPE.sm)
                    .font(font_regular())
                    .color(palette.text()),
            ),
            tooltip::Position::Top,
        )
        .delay(Palette::tooltip_delay())
        .into()
    }
}

fn actions(palette: Palette, app: &App) -> Element<'_, Message> {
    let run = button(text("Run single test").size(TYPE.sm).font(font_medium()))
        .padding([SPACE.s2, SPACE.s4])
        .style(palette.btn_primary())
        .on_press_maybe((!app.round_running).then_some(Message::RunSingleTest));
    let continuous = button(text("Start continuous").size(TYPE.sm).font(font_medium()))
        .padding([SPACE.s2, SPACE.s4])
        .style(palette.btn_secondary())
        .on_press(Message::Resume);
    let mut actions = row![run, continuous].spacing(SPACE.s3).align_y(Center);
    if app.round_running {
        actions = actions.push(text("Measuring").size(TYPE.sm).color(palette.muted()));
    }
    actions.into()
}

fn empty_state<'a>(palette: Palette) -> Element<'a, Message> {
    column![
        text("No rounds yet")
            .size(TYPE.lg)
            .font(font_medium())
            .color(palette.text()),
        text("Run a single test or start continuous mode.")
            .size(TYPE.sm)
            .font(font_regular())
            .color(palette.muted()),
    ]
    .spacing(SPACE.s2)
    .into()
}

fn banner<'a>(palette: Palette, message: &str) -> Element<'a, Message> {
    palette.well_frame(
        text(message.to_string())
            .size(TYPE.sm)
            .font(font_regular())
            .color(palette.danger()),
    )
}

struct ActivityStrip {
    palette: Palette,
    frame: Option<ActivityFrame>,
}

impl canvas::Program<Message> for ActivityStrip {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let Some(activity) = self.frame else {
            return Vec::new();
        };
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let mut color: Color = self.palette.accent();
        color.a *= activity.alpha;
        frame.stroke(
            &canvas::Path::line(
                Point::new(activity.offset, 1.0),
                Point::new(bounds.width + activity.offset, 1.0),
            ),
            canvas::Stroke::default().with_width(2.0).with_color(color),
        );
        vec![frame.into_geometry()]
    }
}

fn round_values(round: &LatestRound) -> [MetricValue; 5] {
    match round {
        LatestRound::Live(outcome) => live_values(&outcome.result),
        LatestRound::Stored(row) => stored_values(row),
    }
}

fn live_values(result: &RoundResult) -> [MetricValue; 5] {
    [
        throughput_value(result.down, "download", result.skipped_reason.as_deref()),
        throughput_value(result.up, "upload", result.skipped_reason.as_deref()),
        ping_idle_value(result.ping_idle, result.skipped_reason.as_deref()),
        ping_under_load_value(
            result.ping_down,
            result.ping_up,
            result.down_load,
            result.up_load,
        ),
        loss_value(result.ping_idle, result.skipped_reason.as_deref()),
    ]
}

fn stored_values(stored: &alidade_store::RoundRow) -> [MetricValue; 5] {
    let reason = stored.skipped_reason.as_deref();
    let download = stored_speed(stored.down_bps, "download", reason);
    let upload = stored_speed(stored.up_bps, "upload", reason);
    let idle = match (
        stored.ping_idle_ms,
        stored.loss_idle_pct.or(stored.loss_pct),
    ) {
        (Some(avg), _) => MetricValue::Value(format!("{avg:.1}")),
        (None, Some(loss)) => MetricValue::Missing(format!("no answer ({loss:.0}% loss)")),
        (None, None) => MetricValue::Missing(missing_reason(reason, "ping")),
    };
    let load = match super::ping_load_ms(stored) {
        Some(avg) => MetricValue::Value(format!("{avg:.1}")),
        None => MetricValue::Missing(stored_load_reason(stored)),
    };
    let loss = match stored.loss_idle_pct.or(stored.loss_pct) {
        Some(loss) => MetricValue::Value(format!("{loss:.2}")),
        None if stored.ping_idle_ms.is_some() => {
            MetricValue::Missing("loss not stored for this round".into())
        }
        None => MetricValue::Missing(missing_reason(reason, "ping")),
    };
    [download, upload, idle, load, loss]
}

fn stored_speed(value: Option<f64>, label: &str, reason: Option<&str>) -> MetricValue {
    match value {
        Some(bps) => MetricValue::Value(format!("{:.2}", bps / 1_000_000.0)),
        None => MetricValue::Missing(missing_reason(reason, label)),
    }
}

fn stored_load_reason(stored: &alidade_store::RoundRow) -> String {
    if let Some(loss) = stored.loss_down_pct.or(stored.loss_up_pct) {
        return format!("no answer ({loss:.0}% loss)");
    }
    let window = stored
        .load_down_ms
        .map(|ms| (ms, stored.ping_down_samples))
        .or_else(|| stored.load_up_ms.map(|ms| (ms, stored.ping_up_samples)));
    if let Some((ms, samples)) = window {
        let samples = samples.map_or_else(
            || "sample count not stored".into(),
            |count| format!("{count} sample(s)"),
        );
        return format!(
            "not enough load to measure ({samples} in {:.1}s of load)",
            ms / 1000.0
        );
    }
    if stored.down_bps.or(stored.up_bps).is_some() {
        return "load details not stored for this round".into();
    }
    "no throughput phase ran this round".into()
}

/// A real formatted measurement, or the reason it is missing.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MetricValue {
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
        .and_then(|reasons| {
            reasons
                .split("; ")
                .find(|segment| segment.starts_with(&prefix))
        })
        .map(|segment| segment.trim_start_matches(&prefix).trim().to_string())
        .unwrap_or_else(|| "not selected for this round".to_string())
}

fn throughput_value(
    value: Option<Throughput>,
    label: &str,
    skipped_reason: Option<&str>,
) -> MetricValue {
    match value {
        Some(t) => MetricValue::Value(format!("{:.2}", t.bits_per_sec / 1_000_000.0)),
        None => MetricValue::Missing(missing_reason(skipped_reason, label)),
    }
}

fn ping_idle_value(stats: Option<PingStats>, skipped_reason: Option<&str>) -> MetricValue {
    match stats {
        None => MetricValue::Missing(missing_reason(skipped_reason, "ping")),
        Some(stats) => match stats.avg_ms {
            Some(avg) => MetricValue::Value(format!("{avg:.1}")),
            None => MetricValue::Missing(format!("no answer ({:.0}% loss)", stats.loss_pct)),
        },
    }
}

/// `loss_pct` is always present on a `PingStats` that ran at all (it is the
/// measurement, not a derived average) — so `Some(0.0)` here is a real,
/// measured zero, never a stand-in for "not measured". Only the absence of
/// `PingStats` itself (ping not selected, or no target configured) is
/// `Missing`.
fn loss_value(stats: Option<PingStats>, skipped_reason: Option<&str>) -> MetricValue {
    match stats {
        None => MetricValue::Missing(missing_reason(skipped_reason, "ping")),
        Some(stats) => MetricValue::Value(format!("{:.2}", stats.loss_pct)),
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
) -> MetricValue {
    match (
        down_stats.and_then(|s| s.avg_ms),
        up_stats.and_then(|s| s.avg_ms),
    ) {
        (Some(down), Some(up)) => MetricValue::Value(format!("{:.1}", (down + up) / 2.0)),
        (Some(value), None) | (None, Some(value)) => MetricValue::Value(format!("{value:.1}")),
        (None, None) => MetricValue::Missing(under_load_missing_reason(
            down_stats, up_stats, down_load, up_load,
        )),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn stats(avg_ms: Option<f64>, loss_pct: f64) -> PingStats {
        PingStats {
            avg_ms,
            min_ms: avg_ms,
            max_ms: avg_ms,
            jitter_ms: None,
            loss_pct,
            sent: 5,
        }
    }

    fn throughput(mbit: f64) -> Throughput {
        Throughput {
            bits_per_sec: mbit * 1_000_000.0,
            bytes: 0,
            duration: Duration::from_secs(10),
            capped: false,
        }
    }

    #[test]
    fn a_measured_throughput_is_shown_in_mbit() {
        assert_eq!(
            throughput_value(Some(throughput(123.456)), "download", None),
            MetricValue::Value("123.46".to_string())
        );
    }

    #[test]
    fn a_missing_throughput_shows_the_matching_skip_segment_not_a_zero() {
        let reason = "download: provider: server returned status 429; upload: partial reading discarded (1.2s of 10.0s budget measured, 12% — below the 50% needed to trust the rate)";
        assert_eq!(
            throughput_value(None, "download", Some(reason)),
            MetricValue::Missing("provider: server returned status 429".to_string())
        );
        assert_eq!(
            throughput_value(None, "upload", Some(reason)),
            MetricValue::Missing(
                "partial reading discarded (1.2s of 10.0s budget measured, 12% — below the 50% needed to trust the rate)"
                    .to_string()
            )
        );
    }

    #[test]
    fn a_missing_throughput_with_no_skip_reason_reads_as_not_selected() {
        assert_eq!(
            throughput_value(None, "download", None),
            MetricValue::Missing("not selected for this round".to_string())
        );
    }

    #[test]
    fn ping_idle_reads_the_average_when_something_answered() {
        assert_eq!(
            ping_idle_value(Some(stats(Some(23.4), 0.0)), None),
            MetricValue::Value("23.4".to_string())
        );
    }

    #[test]
    fn ping_idle_with_total_loss_says_so_instead_of_a_fake_average() {
        assert_eq!(
            ping_idle_value(Some(stats(None, 100.0)), None),
            MetricValue::Missing("no answer (100% loss)".to_string())
        );
    }

    #[test]
    fn ping_idle_never_measured_reads_the_skip_reason() {
        assert_eq!(
            ping_idle_value(None, Some("ping: no targets configured")),
            MetricValue::Missing("no targets configured".to_string())
        );
    }

    #[test]
    fn a_real_zero_loss_is_shown_as_zero_never_as_missing() {
        // The exact case this release exists to get right: a measured 0.0%
        // loss must render as a genuine number, not collapse into the same
        // "missing" bucket as a metric that was never measured at all.
        assert_eq!(
            loss_value(Some(stats(Some(10.0), 0.0)), None),
            MetricValue::Value("0.00".to_string())
        );
    }

    #[test]
    fn loss_is_missing_only_when_ping_never_ran() {
        assert_eq!(
            loss_value(None, Some("ping: no targets configured")),
            MetricValue::Missing("no targets configured".to_string())
        );
    }

    #[test]
    fn ping_under_load_averages_both_phases_when_both_answered() {
        let window = LoadWindow {
            duration: Duration::from_secs(10),
            ping_samples: 9,
            bytes: 1,
        };
        assert_eq!(
            ping_under_load_value(
                Some(stats(Some(20.0), 0.0)),
                Some(stats(Some(30.0), 0.0)),
                Some(window),
                Some(window)
            ),
            MetricValue::Value("25.0".to_string())
        );
    }

    #[test]
    fn ping_under_load_uses_whichever_phase_ran_when_only_one_did() {
        let window = LoadWindow {
            duration: Duration::from_secs(10),
            ping_samples: 9,
            bytes: 1,
        };
        assert_eq!(
            ping_under_load_value(Some(stats(Some(20.0), 0.0)), None, Some(window), None),
            MetricValue::Value("20.0".to_string())
        );
    }

    #[test]
    fn ping_under_load_with_total_loss_says_so() {
        let window = LoadWindow {
            duration: Duration::from_secs(10),
            ping_samples: 9,
            bytes: 1,
        };
        assert_eq!(
            ping_under_load_value(Some(stats(None, 100.0)), None, Some(window), None),
            MetricValue::Missing("no answer (100% loss)".to_string())
        );
    }

    #[test]
    fn ping_under_load_with_too_little_load_names_the_sample_count_not_a_skip() {
        let window = LoadWindow {
            duration: Duration::from_secs(2),
            ping_samples: 2,
            bytes: 500,
        };
        assert_eq!(
            ping_under_load_value(None, None, Some(window), None),
            MetricValue::Missing(
                "not enough load to measure (2 sample(s) in 2.0s of load)".to_string()
            )
        );
    }

    #[test]
    fn ping_under_load_on_a_ping_only_round_says_no_throughput_ran() {
        assert_eq!(
            ping_under_load_value(None, None, None, None),
            MetricValue::Missing("no throughput phase ran this round".to_string())
        );
    }
}
