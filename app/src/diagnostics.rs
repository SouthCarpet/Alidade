//! Plain-text diagnostics suitable for ISP support tickets.

use std::cmp::Ordering;
use std::time::SystemTime;

use alidade_engine::Settings;
use alidade_store::RoundRow;

/// Formats selected rounds without reading the database or clipboard.
pub fn summary(rounds: &[RoundRow], range: (SystemTime, SystemTime), app_version: &str) -> String {
    let (from, to) = range;
    let mut lines = vec![
        format!("Alidade v{app_version} diagnostics"),
        format!(
            "Selected range (local time): {} to {}",
            crate::screens::format_time(from),
            crate::screens::format_time(to)
        ),
        format!("Round count: {}", rounds.len()),
    ];

    if rounds.is_empty() {
        lines.push("No rounds in the selected range.".to_string());
        return lines.join("\n");
    }

    lines.extend([
        metric_line(
            "Download",
            rounds
                .iter()
                .filter_map(|row| row.down_bps.map(|bps| bps / 1e6)),
            "Mbit/s",
        ),
        metric_line(
            "Upload",
            rounds
                .iter()
                .filter_map(|row| row.up_bps.map(|bps| bps / 1e6)),
            "Mbit/s",
        ),
        metric_line(
            "Ping idle",
            rounds.iter().filter_map(|row| row.ping_idle_ms),
            "ms",
        ),
        metric_line(
            "Ping under load",
            rounds.iter().filter_map(ping_under_load),
            "ms",
        ),
        metric_line("Loss", rounds.iter().filter_map(|row| row.loss_pct), "%"),
        "Worst rounds:".to_string(),
    ]);

    for (index, row) in worst_rounds(rounds).iter().enumerate() {
        lines.push(format!(
            "{}. {}: download {}, ping under load {}",
            index + 1,
            crate::screens::format_time(row.started_at),
            value(row.down_bps.map(|bps| bps / 1e6), "Mbit/s"),
            value(ping_under_load(row), "ms")
        ));
    }

    let settings = Settings::default();
    lines.push(format!(
        "Measurement method: selected throughput phases are wall-clock bounded to 10 seconds; provider endpoints: download {}; upload {}.",
        settings.endpoints.download_url, settings.endpoints.upload_url
    ));
    lines.join("\n")
}

fn metric_line(label: &str, values: impl Iterator<Item = f64>, unit: &str) -> String {
    let values: Vec<_> = values.collect();
    let avg = (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64);
    let min = values.iter().copied().reduce(f64::min);
    let max = values.iter().copied().reduce(f64::max);
    format!(
        "{label}: avg {}, min {}, max {}",
        value(avg, unit),
        value(min, unit),
        value(max, unit)
    )
}

fn value(number: Option<f64>, unit: &str) -> String {
    number.map_or_else(
        || format!("n/a {unit}"),
        |number| format!("{number:.1} {unit}"),
    )
}

fn ping_under_load(row: &RoundRow) -> Option<f64> {
    match (row.ping_down_ms, row.ping_up_ms) {
        (Some(down), Some(up)) => Some((down + up) / 2.0),
        (Some(ping), None) | (None, Some(ping)) => Some(ping),
        (None, None) => None,
    }
}

fn worst_rounds(rounds: &[RoundRow]) -> Vec<&RoundRow> {
    let mut rows = rounds.iter().collect::<Vec<_>>();
    rows.sort_by(|left, right| match (left.down_bps, right.down_bps) {
        (Some(left), Some(right)) => left.total_cmp(&right),
        (None, None) => ping_under_load(right)
            .unwrap_or(f64::NEG_INFINITY)
            .total_cmp(&ping_under_load(left).unwrap_or(f64::NEG_INFINITY)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
    });
    rows.into_iter().take(3).collect()
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use alidade_engine::RoundKind;

    use super::*;

    fn row(id: i64, seconds: u64, down_mbit: Option<f64>, load_ping: Option<f64>) -> RoundRow {
        RoundRow {
            id,
            started_at: UNIX_EPOCH + Duration::from_secs(seconds),
            mode: RoundKind::Full,
            down_bps: down_mbit.map(|value| value * 1e6),
            up_bps: Some(20e6),
            ping_idle_ms: Some(10.0),
            ping_down_ms: load_ping,
            ping_up_ms: load_ping,
            jitter_ms: None,
            loss_pct: Some(1.0),
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
    fn typical_rounds_report_averages_and_methodology_from_the_brief() {
        // Oracle: the binding brief requires one-decimal metrics and the CSV method wording.
        let report = summary(
            &[
                row(1, 0, Some(100.0), Some(20.0)),
                row(2, 60, Some(200.0), Some(40.0)),
            ],
            (UNIX_EPOCH, UNIX_EPOCH + Duration::from_secs(60)),
            "0.1.0",
        );
        assert!(report.contains("Download: avg 150.0 Mbit/s, min 100.0 Mbit/s, max 200.0 Mbit/s"));
        assert!(report.contains("Ping under load: avg 30.0 ms, min 20.0 ms, max 40.0 ms"));
        assert!(report.contains("wall-clock bounded to 10 seconds"));
    }

    #[test]
    fn missing_metrics_render_as_na_instead_of_zero() {
        // Oracle: the binding brief says missing values print as n/a, never zero.
        let mut missing = row(1, 0, None, None);
        missing.up_bps = None;
        missing.ping_idle_ms = None;
        missing.loss_pct = None;
        let report = summary(&[missing], (UNIX_EPOCH, UNIX_EPOCH), "0.1.0");
        assert!(report.contains("Download: avg n/a Mbit/s, min n/a Mbit/s, max n/a Mbit/s"));
        assert!(report.contains("Loss: avg n/a %, min n/a %, max n/a %"));
    }

    #[test]
    fn fewer_than_three_rounds_lists_every_available_round() {
        // Oracle: the brief asks for the three worst rounds, limited by available rounds.
        let report = summary(
            &[
                row(1, 0, Some(100.0), Some(20.0)),
                row(2, 60, Some(50.0), Some(30.0)),
            ],
            (UNIX_EPOCH, UNIX_EPOCH),
            "0.1.0",
        );
        assert!(report.contains("1. 1970-01-01 00:01: download 50.0 Mbit/s"));
        assert!(report.contains("2. 1970-01-01 00:00: download 100.0 Mbit/s"));
        assert!(!report.contains("3. "));
    }

    #[test]
    fn worst_rounds_sort_download_then_missing_download_by_loaded_ping() {
        // Oracle: the brief defines low download as worst and loaded ping as the fallback.
        let report = summary(
            &[
                row(1, 0, Some(80.0), Some(20.0)),
                row(2, 60, Some(20.0), Some(30.0)),
                row(3, 120, None, Some(70.0)),
                row(4, 180, None, Some(90.0)),
            ],
            (UNIX_EPOCH, UNIX_EPOCH),
            "0.1.0",
        );
        let first = report.find("1. 1970-01-01 00:01");
        let second = report.find("2. 1970-01-01 00:00");
        let third = report.find("3. 1970-01-01 00:03");
        assert!(first < second && second < third);
    }

    #[test]
    fn empty_range_is_explicitly_copyable() {
        // Oracle: the brief requires this exact empty-range sentence.
        let report = summary(&[], (UNIX_EPOCH, UNIX_EPOCH), "0.1.0");
        assert!(report.contains("No rounds in the selected range."));
    }
}
