//! CSV export of stored rounds. Speeds are written in Mbit/s (bits/s ÷ 1e6,
//! two decimal places) because that is what a human reads.

use std::io::Write;
use std::path::Path;

use alidade_engine::{Probe, Settings};

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
    let settings = Settings::default();
    let mut file = std::fs::File::create(out)?;
    for line in methodology(&settings, rows) {
        writeln!(file, "# {line}")?;
    }
    let mut wtr = csv::Writer::from_writer(file);
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

/// Context a recipient needs to assess exported measurements without the
/// application or its database. The endpoints and primary probe come from
/// the shipped settings, which are the only settings this store API receives;
/// settings used for an older round are not persisted with that round.
fn methodology(settings: &Settings, rows: &[RoundRow]) -> Vec<String> {
    let (range_start, range_end) = match (rows.first(), rows.last()) {
        (Some(first), Some(last)) => (format_started_at(first), format_started_at(last)),
        _ => ("no rows".to_string(), "no rows".to_string()),
    };
    let primary_probe = settings
        .targets
        .iter()
        .find(|target| target.enabled)
        .map(|target| match &target.probe {
            Probe::Icmp { host } => format!("ICMP echo to {host}"),
            Probe::TcpConnect { host, port } => format!("TCP connect to {host}:{port}"),
        })
        .unwrap_or_else(|| "no enabled target".to_string());

    vec![
        format!("Alidade v{} evidence export.", env!("CARGO_PKG_VERSION")),
        "Selected throughput phases are wall-clock bounded to 10 seconds; the transfer is whatever fits in that time.".to_string(),
        format!(
            "Shipped settings provider endpoints: download {}; upload {}.",
            settings.endpoints.download_url, settings.endpoints.upload_url
        ),
        format!("Ping columns use {primary_probe}, the primary enabled target."),
        "started_at is UTC, formatted as RFC 3339 with Z.".to_string(),
        format!("Rows: {}; exported started_at range: {range_start} to {range_end}.", rows.len()),
        "Blank down_mbps or up_mbps means the metric was not selected or no trustworthy rate was available; skipped_reason records provider failures and discarded short windows.".to_string(),
        "load_down_ms and load_up_ms are the actual throughput load windows in milliseconds; 207 Mbit/s over 1.2 s and over 10 s are different claims.".to_string(),
        // Settings are not stored per round, so the endpoint and phase-budget
        // lines above describe the CURRENT configuration, not necessarily the
        // one each row was measured under. Saying so is the difference between
        // a header that documents the file and one that quietly overstates it —
        // rows with empty load windows are visibly from an earlier version, and
        // a reader who was not told would take every line above as applying to
        // all of them.
        "Settings are not recorded per row: the endpoint and phase-budget lines above describe the current configuration. Rows measured by an earlier version may differ, and rows with empty load_down_ms/load_up_ms predate those columns.".to_string(),
    ]
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
