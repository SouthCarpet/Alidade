//! History screen (plan Task 4): From/To pickers, rounds table, CSV export.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use design_tokens::scale::{SPACE, TYPE};
use iced::widget::{button, column, container, row, text, text_input};
use iced::{Center, Element, Fill};

use crate::state::{App, Message};
use crate::theme::Palette;
use crate::ui::{font_medium, font_regular};

use super::{days_from_civil, format_ymd, newest_rows, rounds_table, with_toast};

pub fn view(app: &App) -> Element<'_, Message> {
    let palette = Palette::light();
    let from = picker(
        palette,
        "From",
        &app.history_from,
        Message::HistoryFromChanged,
    );
    let to = picker(palette, "To", &app.history_to, Message::HistoryToChanged);
    let export = button(text("Export CSV").size(TYPE.sm).font(font_medium()))
        .padding([SPACE.s2, SPACE.s4])
        .style(palette.btn_primary())
        .on_press(Message::ExportCsv);

    let shown = newest_rows(&app.history_rows, app.history_rows.len());

    let body = column![
        row![from, to, export].spacing(SPACE.s4).align_y(Center),
        text(format!("{} rounds in range", app.history_rows.len()))
            .size(TYPE.xs)
            .font(font_regular())
            .color(palette.muted()),
        rounds_table(palette, &shown),
    ]
    .spacing(SPACE.s4)
    .width(Fill);

    with_toast(
        palette,
        app,
        container(body).width(Fill).height(Fill).into(),
    )
}

fn picker<'a>(
    palette: Palette,
    label: &'static str,
    value: &str,
    on_input: fn(String) -> Message,
) -> Element<'a, Message> {
    column![
        text(label)
            .size(TYPE.xs)
            .font(font_medium())
            .color(palette.muted()),
        text_input("YYYY-MM-DD", value)
            .size(TYPE.sm)
            .padding([SPACE.s2, SPACE.s3])
            .style(palette.text_input())
            .on_input(on_input),
    ]
    .spacing(SPACE.s1)
    .width(Fill)
    .into()
}

/// Default From/To: the last seven UTC calendar days, inclusive of today.
pub fn default_history_range(now: SystemTime) -> (String, String) {
    let to = format_ymd(now);
    let from = format_ymd(
        now.checked_sub(Duration::from_secs(6 * 86_400))
            .unwrap_or(UNIX_EPOCH),
    );
    (from, to)
}

pub fn parse_history_bounds(
    from_text: &str,
    to_text: &str,
) -> Result<(SystemTime, SystemTime), String> {
    let from = parse_day_start(from_text)?;
    let to = parse_day_end(to_text)?;
    if from > to {
        return Err("From must be on or before To".to_string());
    }
    Ok((from, to))
}

pub(crate) fn parse_day_start(value: &str) -> Result<SystemTime, String> {
    let days = parse_ymd_days(value)?;
    let secs =
        u64::try_from(days * 86_400).map_err(|_| "date is before the Unix epoch".to_string())?;
    Ok(UNIX_EPOCH + Duration::from_secs(secs))
}

pub(crate) fn parse_day_end(value: &str) -> Result<SystemTime, String> {
    let start = parse_day_start(value)?;
    Ok(start + Duration::from_secs(86_400 - 1))
}

fn parse_ymd_days(value: &str) -> Result<i64, String> {
    let mut parts = value.split('-');
    let year = parts
        .next()
        .and_then(|p| p.parse::<i32>().ok())
        .ok_or_else(|| format!("expected YYYY-MM-DD, got `{value}`"))?;
    let month = parts
        .next()
        .and_then(|p| p.parse::<u32>().ok())
        .ok_or_else(|| format!("expected YYYY-MM-DD, got `{value}`"))?;
    let day = parts
        .next()
        .and_then(|p| p.parse::<u32>().ok())
        .ok_or_else(|| format!("expected YYYY-MM-DD, got `{value}`"))?;
    if parts.next().is_some() {
        return Err(format!("expected YYYY-MM-DD, got `{value}`"));
    }
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(format!("not a calendar date: `{value}`"));
    }
    Ok(days_from_civil(year, month, day))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_epoch_parses_as_day_zero() {
        let start = parse_day_start("1970-01-01").unwrap();
        assert_eq!(start, UNIX_EPOCH);
        let end = parse_day_end("1970-01-01").unwrap();
        assert_eq!(end, UNIX_EPOCH + Duration::from_secs(86_400 - 1));
    }

    #[test]
    fn a_known_date_round_trips_through_format_ymd() {
        let start = parse_day_start("2026-08-28").unwrap();
        assert_eq!(format_ymd(start), "2026-08-28");
    }

    #[test]
    fn from_after_to_is_rejected() {
        let err = parse_history_bounds("2026-08-28", "2026-08-01").unwrap_err();
        assert!(err.contains("From"));
    }

    #[test]
    fn garbage_is_rejected_without_a_panic() {
        assert!(parse_day_start("28/08/2026").is_err());
        assert!(parse_day_start("").is_err());
        assert!(parse_day_start("2026-13-01").is_err());
    }

    #[test]
    fn default_range_is_seven_inclusive_days() {
        let now = parse_day_start("2026-08-28").unwrap() + Duration::from_secs(12 * 3600);
        let (from, to) = default_history_range(now);
        assert_eq!(to, "2026-08-28");
        assert_eq!(from, "2026-08-22");
    }
}
