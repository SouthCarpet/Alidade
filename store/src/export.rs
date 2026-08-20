//! CSV export of stored rounds. Speeds are written in Mbit/s (bits/s ÷ 1e6,
//! two decimal places) because that is what a human reads.

use std::path::Path;

use crate::{unix_secs, RoundRow, StoreError};

/// `mode` and `capped` are here because they change how the rest of the row
/// must be read: a `ping` row was never asked to measure throughput, and a
/// `capped` row's speeds are truncated by the data budget rather than
/// measured over the full window. `skipped_reason` is here because a blank
/// `down_mbps`/`up_mbps` cell is otherwise the weakest possible evidence: it
/// cannot say whether nothing was asked for, the server refused the round,
/// or a reading was measured but discarded as too short to trust (F1) — an
/// export that hides any of these presents a short or missing measurement
/// as an ordinary blank.
const HEADER: [&str; 13] = [
    "started_at",
    "mode",
    "down_mbps",
    "up_mbps",
    "ping_idle_ms",
    "ping_down_ms",
    "ping_up_ms",
    "jitter_ms",
    "loss_pct",
    "capped",
    "skipped_reason",
    // How long each throughput phase actually pushed data, in ms. Exported
    // because a speed alone cannot be judged: 207.37 Mbit/s measured over
    // 1.2 s of an intended 10 s window is not the same claim as the same
    // figure measured over the full window, and a reader of this file has no
    // other way to tell them apart. Rounds written before the engine gained
    // a trustworthiness rule are exactly the ones that need this column, and
    // they cannot be corrected after the fact without asserting a phase
    // budget the row never recorded — so the context is supplied instead of
    // the number being rewritten.
    "load_down_ms",
    "load_up_ms",
];

/// Write `rows` to `out` with the round-export header. Returns the number
/// of data rows written (not counting the header).
pub fn write_rounds_csv(rows: &[RoundRow], out: &Path) -> Result<usize, StoreError> {
    let mut wtr = csv::Writer::from_path(out)?;
    wtr.write_record(HEADER)?;
    for row in rows {
        wtr.write_record([
            format_started_at(row),
            row.mode.as_str().to_string(),
            mbps(row.down_bps),
            mbps(row.up_bps),
            opt_f64(row.ping_idle_ms),
            opt_f64(row.ping_down_ms),
            opt_f64(row.ping_up_ms),
            opt_f64(row.jitter_ms),
            opt_f64(row.loss_pct),
            row.capped.to_string(),
            row.skipped_reason.clone().unwrap_or_default(),
            opt_f64(row.load_down_ms),
            opt_f64(row.load_up_ms),
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
