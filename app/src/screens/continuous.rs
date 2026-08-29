//! Continuous screen (plan Task 4): live chart, time-range chips, rounds
//! table, Pause / Export CSV.

use std::time::{Duration, SystemTime};

use alidade_store::RoundRow;
use design_tokens::scale::{SPACE, TYPE};
use iced::widget::{button, column, container, row, text, Canvas};
use iced::{Center, Element, Fill, Length};

use crate::chart::{LineChart, TimeRange};
use crate::state::{App, Message};
use crate::theme::Palette;
use crate::ui::{font_medium, font_regular};

use super::{
    countdown_label, newest_rows, rounds_table, status_pill, with_toast, TABLE_VISIBLE_ROWS,
};

const RANGE_CHIPS: [(TimeRange, &str); 4] = [
    (TimeRange::Hour, "1 h"),
    (TimeRange::SixHours, "6 h"),
    (TimeRange::Day, "24 h"),
    (TimeRange::Week, "7 d"),
];

// Task 7 pre-work A1: `palette` now comes from the caller instead of a
// hardcoded `Palette::light()`, and is threaded into the chart too.
pub fn view(app: &App, palette: Palette) -> Element<'_, Message> {
    let table = table_rows(&app.chart_rows);

    let chart: Canvas<LineChart, Message> = Canvas::new(LineChart::with_palette(
        &app.chart_rows,
        app.time_range,
        palette,
    ))
    .width(Fill)
    .height(Length::Fixed(SPACE.s6 * 10.0));

    let mut sections: Vec<Element<'_, Message>> = Vec::new();
    if let Some(err) = &app.load_error {
        sections.push(
            text(err.clone())
                .size(TYPE.sm)
                .font(font_regular())
                .color(palette.danger())
                .into(),
        );
    }
    sections.push(actions(palette, app));
    sections.push(range_chips(palette, app.time_range));
    sections.push(palette.card(true, container(chart).width(Fill)));
    sections.push(rounds_table(palette, &table));
    let body = column(sections).spacing(SPACE.s4).width(Fill);

    with_toast(
        palette,
        app,
        container(body).width(Fill).height(Fill).into(),
    )
}

fn actions(palette: Palette, app: &App) -> Element<'_, Message> {
    let pause = button(text("Pause").size(TYPE.sm).font(font_medium()))
        .padding([SPACE.s2, SPACE.s4])
        .style(palette.btn_secondary())
        .on_press_maybe(app.continuous_running.then_some(Message::Pause));
    let resume = button(text("Resume").size(TYPE.sm).font(font_medium()))
        .padding([SPACE.s2, SPACE.s4])
        .style(palette.btn_primary())
        .on_press_maybe((!app.continuous_running).then_some(Message::Resume));
    let export = button(text("Export CSV").size(TYPE.sm).font(font_medium()))
        .padding([SPACE.s2, SPACE.s4])
        .style(palette.btn_secondary())
        .on_press(Message::ExportCsv);

    let pill = status_pill(palette, status_text(app));
    row![pause, resume, export, pill]
        .spacing(SPACE.s3)
        .align_y(Center)
        .into()
}

fn range_chips<'a>(palette: Palette, active: TimeRange) -> Element<'a, Message> {
    let chips = RANGE_CHIPS
        .into_iter()
        .map(|(range, label)| chip(palette, range, label, range == active));
    row(chips).spacing(SPACE.s2).into()
}

fn chip<'a>(
    palette: Palette,
    range: TimeRange,
    label: &'static str,
    selected: bool,
) -> Element<'a, Message> {
    let label = text(label).size(TYPE.xs).font(font_medium());
    if selected {
        button(label)
            .padding([SPACE.s1, SPACE.s3])
            .style(palette.btn_primary())
            .on_press(Message::RangeSelected(range))
            .into()
    } else {
        button(label)
            .padding([SPACE.s1, SPACE.s3])
            .style(palette.btn_secondary())
            .on_press(Message::RangeSelected(range))
            .into()
    }
}

fn status_text(app: &App) -> String {
    if app.continuous_paused || !app.continuous_running {
        return "Paused".to_string();
    }
    let now = SystemTime::now();
    let deadline = app.scheduler.next_due(now);
    countdown_label(now, deadline, true)
}

/// Duration each time-range chip covers. Kept next to the chips so a new
/// chip cannot ship without a matching window.
pub(crate) fn range_duration(range: TimeRange) -> Duration {
    match range {
        TimeRange::Hour => Duration::from_secs(60 * 60),
        TimeRange::SixHours => Duration::from_secs(6 * 60 * 60),
        TimeRange::Day => Duration::from_secs(24 * 60 * 60),
        TimeRange::Week => Duration::from_secs(7 * 24 * 60 * 60),
    }
}

/// The rows the rounds table renders: the newest [`TABLE_VISIBLE_ROWS`]
/// loaded rounds, independent of the chart's own active range (F3, plan 052
/// Task 7 fix wave). Cause: the table used to be filtered by `bucket` at the
/// same range as the chart, so the default "1 h" range only ever held the
/// two or three rounds that fell in the last hour, even though `chart_rows`
/// already held a full week and the well beneath it was sized for eight.
pub(crate) fn table_rows(chart_rows: &[RoundRow]) -> Vec<RoundRow> {
    newest_rows(chart_rows, TABLE_VISIBLE_ROWS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::newest_rows;
    use alidade_engine::RoundKind;
    use std::time::UNIX_EPOCH;

    fn stub(id: i64, secs: u64) -> RoundRow {
        RoundRow {
            id,
            started_at: UNIX_EPOCH + Duration::from_secs(secs),
            mode: RoundKind::Full,
            down_bps: Some(100_000_000.0),
            up_bps: None,
            ping_idle_ms: Some(20.0),
            ping_down_ms: None,
            ping_up_ms: None,
            jitter_ms: None,
            loss_pct: Some(0.0),
            jitter_idle_ms: None,
            jitter_down_ms: None,
            jitter_up_ms: None,
            loss_idle_pct: None,
            loss_down_pct: None,
            loss_up_pct: None,
            bytes_down: 0,
            bytes_up: 0,
            load_down_ms: None,
            load_up_ms: None,
            ping_down_samples: None,
            ping_up_samples: None,
            capped: false,
            skipped_reason: None,
        }
    }

    /// F3 (plan 052 Task 7 fix wave): reproduces the reported "3 rows
    /// instead of 8" shape — 60 rounds spread evenly over 24h, the same
    /// density `seed_demo` writes. The default "1 h" chart range would only
    /// ever cover the last two or three of these, but the table must show
    /// the newest eight regardless of the active range.
    #[test]
    fn table_rows_shows_the_newest_eight_even_though_the_default_hour_range_would_show_far_fewer() {
        let rows: Vec<_> = (0..60).map(|i| stub(i, i as u64 * 24 * 60)).collect();
        let shown = table_rows(&rows);
        assert_eq!(
            shown.len(),
            TABLE_VISIBLE_ROWS,
            "must show eight regardless of how few rounds a 1h range would keep"
        );
        assert_eq!(shown[0].id, 59, "newest round first");
        assert_eq!(shown[7].id, 52, "eighth-newest round last");
    }

    #[test]
    fn table_shows_at_most_eight_newest() {
        let rows: Vec<_> = (0..12).map(|i| stub(i, 1_000 + i as u64 * 60)).collect();
        let shown = newest_rows(&rows, 8);
        assert_eq!(shown.len(), 8);
        assert_eq!(shown[0].id, 11);
        assert_eq!(shown[7].id, 4);
    }

    #[test]
    fn range_durations_match_the_chip_labels() {
        assert_eq!(range_duration(TimeRange::Hour), Duration::from_secs(3600));
        assert_eq!(
            range_duration(TimeRange::SixHours),
            Duration::from_secs(6 * 3600)
        );
        assert_eq!(
            range_duration(TimeRange::Day),
            Duration::from_secs(24 * 3600)
        );
        assert_eq!(
            range_duration(TimeRange::Week),
            Duration::from_secs(7 * 24 * 3600)
        );
    }
}
