//! Schema migration via SQLite `PRAGMA user_version`.
//!
//! Version 1 is the initial schema: `rounds`, `ping_samples`, `ping_minute`.
//! `migrate` is idempotent — opening an already-migrated file is a no-op.

use rusqlite::Connection;

use crate::StoreError;

pub const SCHEMA_VERSION: i32 = 1;

const V1: &str = "
CREATE TABLE IF NOT EXISTS rounds (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at INTEGER NOT NULL,
    down_bps REAL,
    up_bps REAL,
    ping_idle_ms REAL,
    ping_down_ms REAL,
    ping_up_ms REAL,
    jitter_ms REAL,
    loss_pct REAL,
    bytes_down INTEGER NOT NULL,
    bytes_up INTEGER NOT NULL,
    skipped_reason TEXT
);
CREATE INDEX IF NOT EXISTS idx_rounds_started_at ON rounds(started_at);

CREATE TABLE IF NOT EXISTS ping_samples (
    target TEXT NOT NULL,
    at INTEGER NOT NULL,
    rtt_ms REAL
);
CREATE INDEX IF NOT EXISTS idx_ping_samples_target_at ON ping_samples(target, at);

CREATE TABLE IF NOT EXISTS ping_minute (
    target TEXT NOT NULL,
    minute INTEGER NOT NULL,
    avg_ms REAL,
    min_ms REAL,
    max_ms REAL,
    loss_pct REAL,
    PRIMARY KEY (target, minute)
);
";

/// Apply pending migrations. Safe to call on every `Store::open`.
pub fn migrate(conn: &Connection) -> Result<(), StoreError> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version == SCHEMA_VERSION {
        return Ok(());
    }
    if version > SCHEMA_VERSION {
        return Err(StoreError::UnsupportedSchema(version));
    }

    // version == 0 (fresh file). Apply v1 atomically so a crash cannot
    // leave tables without the matching user_version.
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(V1)?;
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()?;
    Ok(())
}
