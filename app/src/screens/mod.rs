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
#[path = "../drawing.rs"]
pub mod drawing;
pub mod help;
pub mod history;
pub mod home;
pub mod ping_monitor;
pub mod settings;

pub(crate) const TABLE_VISIBLE_ROWS: usize = 8;

pub(crate) fn page_head<'a>(
    palette: Palette,
    title: &'static str,
    metadata: String,
) -> Element<'a, Message> {
    let metadata = metadata.split_whitespace().collect::<Vec<_>>().join(" ");
    column![
        text(title)
            .size(TYPE.xxxl)
            .font(iced::Font {
                weight: iced::font::Weight::Semibold,
                ..font_regular()
            })
            .color(palette.text()),
        text(metadata)
            .size(TYPE.sm)
            .font(font_numeric())
            .color(palette.muted())
            .wrapping(text::Wrapping::None),
    ]
    .spacing(SPACE.s1)
    .width(Fill)
    .into()
}

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

/// C5 (Task 4 verification): the render cadence is `MOTION.live` (1200 ms),
/// not 1000 ms, so a caller that tracked "seconds remaining" by counting
/// ticks would sometimes advance by two seconds in one tick and sometimes by
/// none — a real drift, not just a display quirk. Taking `now` and the
/// absolute `deadline` and subtracting fresh on every call means each label
/// is always exactly `deadline - now` at the moment it is drawn: a slow or
/// skipped tick changes how often the label updates, never what number it
/// shows.
pub(crate) fn countdown_label(now: SystemTime, deadline: SystemTime, running: bool) -> String {
    let remaining = deadline.duration_since(now).unwrap_or(Duration::ZERO);
    let secs = remaining.as_secs();
    let mm = secs / 60;
    let ss = secs % 60;
    if running {
        format!("Measuring · next round in {mm:02}:{ss:02}")
    } else {
        format!("Next round in {mm:02}:{ss:02}")
    }
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

#[derive(Clone, Copy)]
pub enum RoundColumn {
    Time,
    Down,
    Up,
    PingIdle,
    PingLoad,
    Jitter,
    Loss,
}

impl RoundColumn {
    pub fn alignment(self) -> iced::alignment::Horizontal {
        match self {
            Self::Time => iced::alignment::Horizontal::Left,
            _ => iced::alignment::Horizontal::Right,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Time => "Time",
            Self::Down => "Down",
            Self::Up => "Up",
            Self::PingIdle => "Ping idle",
            Self::PingLoad => "Ping load",
            Self::Jitter => "Jitter",
            Self::Loss => "Loss",
        }
    }
}

fn round_cell(
    palette: Palette,
    column: RoundColumn,
    portion: u16,
    value: Option<String>,
) -> iced::widget::Text<'static> {
    let header = value.is_none();
    text(value.unwrap_or_else(|| column.label().into()))
        .align_x(column.alignment())
        .width(Length::FillPortion(portion))
        .size(if header { TYPE.xs } else { TYPE.sm })
        .font(if header {
            font_medium()
        } else if matches!(column, RoundColumn::Time) {
            font_regular()
        } else {
            font_numeric()
        })
        .color(if header {
            palette.muted()
        } else {
            palette.text()
        })
}

fn header_cell(palette: Palette, column: RoundColumn, portion: u16) -> iced::widget::Text<'static> {
    round_cell(palette, column, portion, None)
}

fn time_cell(palette: Palette, value: String, portion: u16) -> iced::widget::Text<'static> {
    round_cell(palette, RoundColumn::Time, portion, Some(value))
}

fn numeric_cell(
    palette: Palette,
    value: String,
    portion: u16,
    column: RoundColumn,
) -> iced::widget::Text<'static> {
    round_cell(palette, column, portion, Some(value))
}

#[cfg(test)]
mod round_cell_tests {
    use super::*;
    use iced::advanced::{
        layout, renderer::Headless, text::Renderer as TextRenderer, widget::Tree,
    };
    use iced::alignment::Horizontal;

    fn built_alignment(
        cell: iced::widget::Text<'static>,
        renderer: &iced::Renderer,
    ) -> iced::advanced::text::Alignment {
        let mut element: Element<'static, Message> = cell.into();
        let mut tree = Tree::new(element.as_widget());
        element.as_widget_mut().layout(
            &mut tree,
            renderer,
            &layout::Limits::new(iced::Size::ZERO, iced::Size::new(200.0, 40.0)),
        );
        tree.state
            .downcast_ref::<iced::widget::text::State<<iced::Renderer as TextRenderer>::Paragraph>>(
            )
            .align_x()
    }

    #[tokio::test]
    async fn rounds_numeric_columns_align_headers_and_values_right() {
        // R2-F1: every numeric header and value must build a right-aligned paragraph.
        let renderer =
            iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(16.0), Some("tiny-skia"))
                .await
                .expect("headless text renderer");
        for column in [
            RoundColumn::Down,
            RoundColumn::Up,
            RoundColumn::PingIdle,
            RoundColumn::PingLoad,
            RoundColumn::Jitter,
            RoundColumn::Loss,
        ] {
            let header = header_cell(Palette::light(), column, 2);
            let value = numeric_cell(Palette::light(), "42.9".into(), 2, column);
            assert_eq!(
                built_alignment(header, &renderer),
                Horizontal::Right.into(),
                "numeric header"
            );
            assert_eq!(
                built_alignment(value, &renderer),
                Horizontal::Right.into(),
                "numeric value"
            );
        }
    }

    #[tokio::test]
    async fn time_header_and_value_keep_left_alignment() {
        // The p3 table contract keeps Time headers and timestamp values left-aligned.
        let renderer =
            iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(16.0), Some("tiny-skia"))
                .await
                .expect("headless text renderer");
        assert_eq!(
            built_alignment(
                header_cell(Palette::light(), RoundColumn::Time, 3),
                &renderer
            ),
            Horizontal::Left.into()
        );
        assert_eq!(
            built_alignment(time_cell(Palette::light(), "12:34:56".into(), 3), &renderer),
            Horizontal::Left.into()
        );
    }
}

fn table_header(palette: Palette) -> Element<'static, Message> {
    row![
        header_cell(palette, RoundColumn::Time, 3),
        hairline(palette),
        header_cell(palette, RoundColumn::Down, 2),
        header_cell(palette, RoundColumn::Up, 2),
        hairline(palette),
        header_cell(palette, RoundColumn::PingIdle, 2),
        header_cell(palette, RoundColumn::PingLoad, 2),
        hairline(palette),
        header_cell(palette, RoundColumn::Jitter, 2),
        header_cell(palette, RoundColumn::Loss, 2),
    ]
    .spacing(SPACE.s2)
    .align_y(Alignment::Center)
    .width(Fill)
    .into()
}

fn table_row<'a>(palette: Palette, row: &RoundRow) -> Element<'a, Message> {
    row![
        time_cell(palette, format_time(row.started_at), 3),
        hairline(palette),
        numeric_cell(palette, format_mbit(row.down_bps), 2, RoundColumn::Down),
        numeric_cell(palette, format_mbit(row.up_bps), 2, RoundColumn::Up),
        hairline(palette),
        numeric_cell(
            palette,
            format_ms(row.ping_idle_ms),
            2,
            RoundColumn::PingIdle
        ),
        numeric_cell(
            palette,
            format_ms(ping_load_ms(row)),
            2,
            RoundColumn::PingLoad
        ),
        hairline(palette),
        numeric_cell(palette, format_ms(row.jitter_ms), 2, RoundColumn::Jitter),
        numeric_cell(palette, format_pct(row.loss_pct), 2, RoundColumn::Loss),
    ]
    .spacing(SPACE.s2)
    .align_y(Alignment::Center)
    .width(Fill)
    .into()
}

pub(crate) fn rounds_table<'a>(palette: Palette, rows: &[RoundRow]) -> Element<'a, Message> {
    let row_h = TYPE.sm + SPACE.s2 * 2.0;
    rounds_table_sized(
        palette,
        rows,
        Length::Fixed(row_h * ((TABLE_VISIBLE_ROWS + 1) as f32)),
    )
}

pub(crate) fn adaptive_rounds_table<'a>(
    palette: Palette,
    rows: &[RoundRow],
) -> Element<'a, Message> {
    container(rounds_table_sized(palette, rows, Fill))
        .width(Fill)
        .height(Fill)
        .into()
}

fn rounds_table_sized<'a>(
    palette: Palette,
    rows: &[RoundRow],
    height: Length,
) -> Element<'a, Message> {
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
    // F2 (Task 7 fix wave): a default scrollbar floats over the content, so
    // it covered the rightmost (Loss) column at the table's right edge —
    // in both History and here (Continuous shares this helper). An
    // embedded scrollbar reserves its own layout space instead, so the
    // columns never sit under it.
    let gutter = scrollable::Scrollbar::new().spacing(SPACE.s2);
    palette.well_frame(
        container(
            scrollable(column(body).spacing(SPACE.s2).width(Fill))
                .height(Fill)
                .direction(scrollable::Direction::Vertical(gutter)),
        )
        .height(height)
        .width(Fill),
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

    #[tokio::test]
    async fn table_row_keeps_each_measurement_in_its_own_aligned_column() {
        // R3/M5: distinct values must reach Time/Down/Up/Idle/Load/Jitter/Loss in order.
        // Oracle: the shared seven-column table contract, with Time left and numbers right.
        use iced::advanced::{layout, renderer::Headless, text::Renderer, widget::Tree};
        use iced::alignment::Horizontal::{Left, Right};

        let renderer =
            iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(16.0), Some("tiny-skia"))
                .await
                .expect("headless table renderer");
        let mut row = stub_row(UNIX_EPOCH);
        row.down_bps = Some(94_030_000.0);
        row.up_bps = Some(27_890_000.0);
        row.ping_idle_ms = Some(13.7);
        row.ping_down_ms = Some(40.0);
        row.ping_up_ms = Some(45.8);
        row.jitter_ms = Some(2.3);
        row.loss_pct = Some(1.25);
        let mut element = table_row(Palette::light(), &row);
        let mut tree = Tree::new(element.as_widget());
        element.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(iced::Size::ZERO, iced::Size::new(1200.0, 40.0)),
        );
        let actual: Vec<_> = [0, 2, 3, 5, 6, 8, 9]
            .into_iter()
            .map(|index| {
                let paragraph = tree.children[index].state.downcast_ref::<
                iced::widget::text::State<<iced::Renderer as Renderer>::Paragraph>
            >();
                (paragraph.content().trim(), paragraph.align_x())
            })
            .collect();
        assert_eq!(
            actual,
            [
                ("1970-01-01 00:00", Left.into()),
                ("94.03", Right.into()),
                ("27.89", Right.into()),
                ("13.7", Right.into()),
                ("42.9", Right.into()),
                ("2.3", Right.into()),
                ("1.25", Right.into()),
            ],
            "table row values and column alignment"
        );
    }

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
        let now = UNIX_EPOCH;
        let deadline = now + Duration::from_secs(125);
        let label = countdown_label(now, deadline, true);
        assert_eq!(label, "Measuring · next round in 02:05");
    }

    /// C5: derived fresh from the deadline on every call, so an irregular
    /// 1200 ms tick cadence (including a delayed one, modelled by the last
    /// offset) never produces a number larger than the true remaining time,
    /// and the label always matches an independent `deadline - now`
    /// computation exactly.
    #[test]
    fn countdown_derives_from_the_deadline_so_a_skipped_tick_never_shows_a_wrong_number() {
        let deadline = UNIX_EPOCH + Duration::from_secs(600);
        let offsets_ms = [0u64, 1200, 2400, 3600, 4900, 6300];
        let mut previous_secs = u64::MAX;
        for offset_ms in offsets_ms {
            let now = UNIX_EPOCH + Duration::from_millis(offset_ms);
            let label = countdown_label(now, deadline, true);
            let expected_secs = deadline.duration_since(now).unwrap().as_secs();
            assert_eq!(
                label,
                format!(
                    "Measuring · next round in {:02}:{:02}",
                    expected_secs / 60,
                    expected_secs % 60
                )
            );
            assert!(
                expected_secs <= previous_secs,
                "must never show a number larger than the true remaining time"
            );
            previous_secs = expected_secs;
        }
    }

    #[test]
    fn countdown_never_goes_negative_past_the_deadline() {
        let deadline = UNIX_EPOCH + Duration::from_secs(10);
        let now = UNIX_EPOCH + Duration::from_secs(20);
        let label = countdown_label(now, deadline, true);
        assert_eq!(label, "Measuring · next round in 00:00");
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
