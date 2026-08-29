//! Continuous screen (plan Task 4): live chart, time-range chips, rounds
//! table, Pause / Export CSV.

use std::time::{Duration, SystemTime};

use alidade_store::RoundRow;
use design_tokens::scale::{SPACE, TYPE};
use iced::widget::{button, column, container, row, text, Canvas};
use iced::{Center, Element, Fill, Length};

use crate::chart::{bucket, LineChart, TimeRange};
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

pub fn view(app: &App) -> Element<'_, Message> {
    let palette = Palette::light();
    let filtered = rows_in_range(&app.chart_rows, app.time_range);
    let table = newest_rows(&filtered, TABLE_VISIBLE_ROWS);

    let chart: Canvas<LineChart, Message> =
        Canvas::new(LineChart::new(&app.chart_rows, app.time_range))
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

pub(crate) fn rows_in_range(rows: &[RoundRow], range: TimeRange) -> Vec<RoundRow> {
    bucket(rows, range)
        .into_iter()
        .filter_map(|point| rows.iter().find(|row| row.id == point.id).cloned())
        .collect()
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

    #[test]
    fn hour_chip_drops_rows_older_than_the_latest_minus_one_hour() {
        let latest = 20_000;
        let rows = vec![
            stub(1, latest - 4 * 3600),
            stub(2, latest - 30 * 60),
            stub(3, latest),
        ];
        let kept = rows_in_range(&rows, TimeRange::Hour);
        let ids: Vec<i64> = kept.iter().map(|row| row.id).collect();
        assert_eq!(ids, vec![2, 3]);
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
