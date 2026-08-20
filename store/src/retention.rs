//! Retention: collapse raw ping samples older than N days into 1-minute
//! aggregates, then delete the raw rows that were folded in.

use std::time::SystemTime;

use rusqlite::{params, Connection};

use crate::{unix_secs, StoreError};

/// Group raw `ping_samples` older than `older_than_days` (relative to `now`)
/// by `at/60`, write `ping_minute` rows, then delete those raw samples.
/// One transaction.
///
/// Returns the number of minute-aggregate rows written (inserted or
/// replaced).
pub fn downsample_pings_at(
    conn: &Connection,
    now: SystemTime,
    older_than_days: u32,
) -> Result<usize, StoreError> {
    let raw_cutoff = unix_secs(now)?
        .saturating_sub(i64::from(older_than_days).saturating_mul(86_400));
    // Never split a one-minute bucket across downsample runs: a later
    // upsert for the remainder would otherwise replace the earlier partial
    // aggregate. `unix_secs` is non-negative, but keep pre-epoch values
    // untouched rather than rounding them toward zero.
    let cutoff = if raw_cutoff >= 0 {
        raw_cutoff - raw_cutoff % 60
    } else {
        raw_cutoff
    };

    let tx = conn.unchecked_transaction()?;
    let moved = tx.execute(
        "INSERT INTO ping_minute (target, minute, avg_ms, min_ms, max_ms, loss_pct)
         SELECT
             target,
             at / 60,
             COALESCE(AVG(rtt_ms), 0.0),
             COALESCE(MIN(rtt_ms), 0.0),
             COALESCE(MAX(rtt_ms), 0.0),
             100.0 * SUM(CASE WHEN rtt_ms IS NULL THEN 1.0 ELSE 0.0 END) / COUNT(*)
         FROM ping_samples
         WHERE at < ?1
         GROUP BY target, at / 60
         ON CONFLICT(target, minute) DO UPDATE SET
             avg_ms = excluded.avg_ms,
             min_ms = excluded.min_ms,
             max_ms = excluded.max_ms,
             loss_pct = excluded.loss_pct",
        params![cutoff],
    )?;
    tx.execute("DELETE FROM ping_samples WHERE at < ?1", params![cutoff])?;
    tx.commit()?;
    Ok(moved)
}
