//! Retention: collapse raw ping samples older than N days into 1-minute
//! aggregates, then delete the raw rows that were folded in.

use std::time::SystemTime;

use rusqlite::{params, Connection};

use crate::{unix_secs, StoreError};

/// Group raw `ping_samples` older than `older_than_days` by `at/60`, write
/// `ping_minute` rows, then delete those raw samples. One transaction.
///
/// Returns the number of minute-aggregate rows written (inserted or
/// replaced).
pub fn downsample_pings(conn: &Connection, older_than_days: u32) -> Result<usize, StoreError> {
    let cutoff = unix_secs(SystemTime::now())?
        .saturating_sub(i64::from(older_than_days).saturating_mul(86_400));

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
