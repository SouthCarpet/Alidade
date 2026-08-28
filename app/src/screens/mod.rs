//! Screen modules. Each one owns a `view(app) -> Element<Message>` function
//! (plan Task 2+); `ui.rs` routes to the active screen's module.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use alidade_store::RoundRow;
use design_tokens::scale::{RADIUS, SPACE, TYPE};
use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Border, Color, Element, Fill, Length};

use crate::state::{App, Message, Toast};
use crate::theme::Palette;
use crate::ui::{font_medium, font_numeric, font_regular};

pub mod continuous;
pub mod history;
pub mod home;
pub mod ping_monitor;

pub(crate) const TABLE_VISIBLE_ROWS: usize = 8;

/// How many of the newest in-range rounds the table shows.
pub(crate) fn newest_rows(rows: &[RoundRow], n: usize) -> Vec<RoundRow> {
    let start = rows.len().saturating_sub(n);
    let mut out = rows[start..].to_vec();
    out.reverse();
    out
}

pub(crate) fn ping_load_ms(row: &RoundRow) -> Option<f64> {
    match (row.ping_down_ms, row.ping_up_ms) {
        (Some(down), Some(up)) => Some((down + up) / 2.0),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

/// Throughput in Mbit/s, two decimals, right-padded so the point lines up.
/// Missing stays missing: never `"0.00"`.
pub(crate) fn format_mbit(bps: Option<f64>) -> String {
    match bps {
        Some(value) => format!("{:>7.2}", value / 1_000_000.0),
        None => format!("{:>7}", "—"),
    }
}

/// Latency in ms, one decimal. Missing stays missing.
pub(crate) fn format_ms(ms: Option<f64>) -> String {
    match ms {
        Some(value) => format!("{:>6.1}", value),
        None => format!("{:>6}", "—"),
    }
}

/// Loss in percent, two decimals. A measured zero is `"  0.00"`.
pub(crate) fn format_pct(pct: Option<f64>) -> String {
    match pct {
        Some(value) => format!("{:>6.2}", value),
        None => format!("{:>6}", "—"),
    }
}

pub(crate) fn format_time(time: SystemTime) -> String {
    let seconds = time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let days = seconds / (24 * 60 * 60);
    let clock = seconds % (24 * 60 * 60);
    let hour = clock / (60 * 60);
    let minute = (clock % (60 * 60)) / 60;
    let (year, month, day) = civil_from_days(days as i64);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}")
}

pub(crate) fn format_ymd(time: SystemTime) -> String {
    let seconds = time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let days = seconds / (24 * 60 * 60);
    let (year, month, day) = civil_from_days(days as i64);
    format!("{year:04}-{month:02}-{day:02}")
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    (year + i64::from(month <= 2), month as u32, day as u32)
}

pub(crate) fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let mut year = i64::from(year);
    let month = i64::from(month);
    let day = i64::from(day);
    if month <= 2 {
        year -= 1;
    }
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

pub(crate) fn countdown_label(remaining: Duration, running: bool) -> String {
    let secs = remaining.as_secs();
    let mm = secs / 60;
    let ss = secs % 60;
    if running {
        format!("Measuring · next round in {mm:02}:{ss:02}")
    } else {
        format!("Next round in {mm:02}:{ss:02}")
    }
}

pub(crate) fn status_pill<'a>(palette: Palette, label: String) -> Element<'a, Message> {
    let background = Color::from(palette.theme.accent_soft);
    let foreground = Color::from(palette.theme.accent_soft_foreground);
    container(
        text(label)
            .size(TYPE.xs)
            .font(font_medium())
            .color(foreground),
    )
    .padding([SPACE.s1, SPACE.s3])
    .style(move |_| {
        container::Style::default()
            .background(background)
            .border(Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: iced::border::radius(RADIUS.pill),
            })
    })
    .into()
}

pub(crate) fn toast_view<'a>(palette: Palette, toast: &Toast) -> Element<'a, Message> {
    let dot_color = if toast.ok {
        palette.success()
    } else {
        palette.danger()
    };
    let dot: Element<'a, Message> = container(
        Space::new()
            .width(Length::Fixed(8.0))
            .height(Length::Fixed(8.0)),
    )
    .style(move |_| {
        container::Style::default()
            .background(dot_color)
            .border(Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: iced::border::radius(RADIUS.pill),
            })
    })
    .into();
    let dismiss = button(text("Dismiss").size(TYPE.xs).font(font_medium()))
        .padding([SPACE.s1, SPACE.s3])
        .style(palette.btn_ghost())
        .on_press(Message::DismissToast);
    palette.well_frame(
        row![
            dot,
            text(toast.message.clone())
                .size(TYPE.sm)
                .color(palette.text()),
            Space::new().width(Fill),
            dismiss,
        ]
        .spacing(SPACE.s2)
        .align_y(Alignment::Center),
    )
}

fn hairline<'a>(palette: Palette) -> Element<'a, Message> {
    container(
        Space::new()
            .width(Length::Fixed(1.0))
            .height(Length::Fixed(TYPE.sm + SPACE.s2)),
    )
    .style(move |_| container::Style::default().background(palette.border()))
    .into()
}

fn header_cell<'a>(palette: Palette, label: &'static str, portion: u16) -> Element<'a, Message> {
    text(label)
        .size(TYPE.xs)
        .font(font_medium())
        .color(palette.muted())
        .width(Length::FillPortion(portion))
        .into()
}

fn time_cell<'a>(palette: Palette, value: String, portion: u16) -> Element<'a, Message> {
    text(value)
        .size(TYPE.sm)
        .font(font_regular())
        .color(palette.text())
        .width(Length::FillPortion(portion))
        .into()
}

fn numeric_cell<'a>(palette: Palette, value: String, portion: u16) -> Element<'a, Message> {
    text(value)
        .size(TYPE.sm)
        .font(font_numeric())
        .color(palette.text())
        .align_x(iced::alignment::Horizontal::Right)
        .width(Length::FillPortion(portion))
        .into()
}

fn table_header(palette: Palette) -> Element<'static, Message> {
    row![
        header_cell(palette, "Time", 3),
        hairline(palette),
        header_cell(palette, "Down", 2),
        header_cell(palette, "Up", 2),
        hairline(palette),
        header_cell(palette, "Ping idle", 2),
        header_cell(palette, "Ping load", 2),
        hairline(palette),
        header_cell(palette, "Jitter", 2),
        header_cell(palette, "Loss", 2),
    ]
    .spacing(SPACE.s2)
    .align_y(Alignment::Center)
    .into()
}

fn table_row<'a>(palette: Palette, row: &RoundRow) -> Element<'a, Message> {
    row![
        time_cell(palette, format_time(row.started_at), 3),
        hairline(palette),
        numeric_cell(palette, format_mbit(row.down_bps), 2),
        numeric_cell(palette, format_mbit(row.up_bps), 2),
        hairline(palette),
        numeric_cell(palette, format_ms(row.ping_idle_ms), 2),
        numeric_cell(palette, format_ms(ping_load_ms(row)), 2),
        hairline(palette),
        numeric_cell(palette, format_ms(row.jitter_ms), 2),
        numeric_cell(palette, format_pct(row.loss_pct), 2),
    ]
    .spacing(SPACE.s2)
    .align_y(Alignment::Center)
    .into()
}

pub(crate) fn rounds_table<'a>(palette: Palette, rows: &[RoundRow]) -> Element<'a, Message> {
    let mut body: Vec<Element<'a, Message>> = vec![table_header(palette)];
    if rows.is_empty() {
        body.push(
            text("No rounds in this range.")
                .size(TYPE.sm)
                .font(font_regular())
                .color(palette.muted())
                .into(),
        );
    } else {
        for row in rows {
            body.push(table_row(palette, row));
        }
    }
    let row_h = TYPE.sm + SPACE.s2 * 2.0;
    let height = row_h * ((TABLE_VISIBLE_ROWS + 1) as f32);
    palette.well_frame(
        scrollable(column(body).spacing(SPACE.s2).width(Fill)).height(Length::Fixed(height)),
    )
}

pub(crate) fn with_toast<'a>(
    palette: Palette,
    app: &App,
    content: Element<'a, Message>,
) -> Element<'a, Message> {
    match &app.toast {
        Some(toast) => column![toast_view(palette, toast), content]
            .spacing(SPACE.s4)
            .width(Fill)
            .into(),
        None => content,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_throughput_is_not_a_zero() {
        assert_eq!(format_mbit(None).trim(), "—");
        assert!(!format_mbit(None).contains('0'));
    }

    #[test]
    fn a_measured_zero_loss_is_shown_as_zero() {
        assert_eq!(format_pct(Some(0.0)).trim(), "0.00");
    }

    #[test]
    fn newest_rows_are_newest_first_and_capped() {
        let t0 = UNIX_EPOCH + Duration::from_secs(10);
        let t1 = UNIX_EPOCH + Duration::from_secs(20);
        let t2 = UNIX_EPOCH + Duration::from_secs(30);
        let rows = vec![stub_row(t0), stub_row(t1), stub_row(t2)];
        let newest = newest_rows(&rows, 2);
        assert_eq!(newest.len(), 2);
        assert_eq!(newest[0].started_at, t2);
        assert_eq!(newest[1].started_at, t1);
    }

    #[test]
    fn countdown_uses_the_plan_copy_when_running() {
        let label = countdown_label(Duration::from_secs(125), true);
        assert_eq!(label, "Measuring · next round in 02:05");
    }

    fn stub_row(started_at: SystemTime) -> RoundRow {
        RoundRow {
            id: 1,
            started_at,
            mode: alidade_engine::RoundKind::Full,
            down_bps: None,
            up_bps: None,
            ping_idle_ms: None,
            ping_down_ms: None,
            ping_up_ms: None,
            jitter_ms: None,
            loss_pct: None,
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
}
