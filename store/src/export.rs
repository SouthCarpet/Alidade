//! CSV export of stored rounds. Speeds are written in Mbit/s (bits/s ÷ 1e6,
//! two decimal places) because that is what a human reads.

use std::path::Path;

use crate::{unix_secs, RoundRow, StoreError};

const HEADER: [&str; 8] = [
    "started_at",
    "down_mbps",
    "up_mbps",
    "ping_idle_ms",
    "ping_down_ms",
    "ping_up_ms",
    "jitter_ms",
    "loss_pct",
];

/// Write `rows` to `out` with the round-export header. Returns the number
/// of data rows written (not counting the header).
pub fn write_rounds_csv(rows: &[RoundRow], out: &Path) -> Result<usize, StoreError> {
    let mut wtr = csv::Writer::from_path(out)?;
    wtr.write_record(HEADER)?;
    for row in rows {
        wtr.write_record([
            format_started_at(row),
            mbps(row.down_bps),
            mbps(row.up_bps),
            opt_f64(row.ping_idle_ms),
            opt_f64(row.ping_down_ms),
            opt_f64(row.ping_up_ms),
            opt_f64(row.jitter_ms),
            opt_f64(row.loss_pct),
        ])?;
    }
    wtr.flush()?;
    Ok(rows.len())
}

fn format_started_at(row: &RoundRow) -> String {
    match unix_secs(row.started_at) {
        Ok(secs) => chrono::DateTime::from_timestamp(secs, 0)
            .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
            .unwrap_or_else(|| secs.to_string()),
        Err(_) => String::new(),
    }
}

fn mbps(bps: Option<f64>) -> String {
    bps.map(|v| format!("{:.2}", v / 1e6)).unwrap_or_default()
}

fn opt_f64(v: Option<f64>) -> String {
    v.map(|x| format!("{x}")).unwrap_or_default()
}
