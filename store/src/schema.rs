//! Schema migration via SQLite `PRAGMA user_version`.
//!
//! Version 1 is the initial schema: `rounds`, `ping_samples`, `ping_minute`.
//! Version 2 adds per-phase jitter/loss on `rounds` so the bufferbloat
//! signal (idle vs under-load) is not collapsed into the idle baseline.
//! `migrate` is idempotent — opening an already-migrated file is a no-op.

use rusqlite::Connection;

use crate::StoreError;

pub const SCHEMA_VERSION: i32 = 2;

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

const V2: &str = "
ALTER TABLE rounds ADD COLUMN jitter_idle_ms REAL;
ALTER TABLE rounds ADD COLUMN jitter_down_ms REAL;
ALTER TABLE rounds ADD COLUMN jitter_up_ms REAL;
ALTER TABLE rounds ADD COLUMN loss_idle_pct REAL;
ALTER TABLE rounds ADD COLUMN loss_down_pct REAL;
ALTER TABLE rounds ADD COLUMN loss_up_pct REAL;
UPDATE rounds SET jitter_idle_ms = jitter_ms WHERE jitter_idle_ms IS NULL;
UPDATE rounds SET loss_idle_pct = loss_pct WHERE loss_idle_pct IS NULL;
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

    // Apply each pending step in one transaction so a crash cannot leave
    // tables without the matching user_version.
    let tx = conn.unchecked_transaction()?;
    if version < 1 {
        tx.execute_batch(V1)?;
    }
    if version < 2 {
        tx.execute_batch(V2)?;
    }
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()?;
    Ok(())
}
