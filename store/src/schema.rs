//! Schema migration via SQLite `PRAGMA user_version`.
//!
//! Version 1 is the initial schema: `rounds`, `ping_samples`, `ping_minute`.
//! Version 2 adds per-phase jitter/loss on `rounds` so the bufferbloat
//! signal (idle vs under-load) is not collapsed into the idle baseline.
//! Version 3 adds `mode` (spec §4, decision D5a), `capped` (D6) and the
//! load-window columns that say what an under-load ping is worth.
//! `migrate` is idempotent — opening an already-migrated file is a no-op.

use rusqlite::Connection;

use crate::StoreError;

pub const SCHEMA_VERSION: i32 = 3;

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

/// The column spec §4 always named and the implementation dropped: which
/// cadence produced the round (`full` | `ping`, decision D5a), whether a
/// byte ceiling truncated it (D6), and how much load the under-load pings
/// actually saw.
///
/// `mode` is `NOT NULL DEFAULT 'full'`, and that default IS the backfill:
/// every round stored before this migration ran the full D4 sequence, so
/// `full` is not a guess. Same for `capped` — nothing before this version
/// could be capped by a data budget, because nothing recorded one.
///
/// `load_*_ms` / `ping_*_samples` stay NULL on those old rows. That is
/// correct rather than convenient: nobody measured the load window then, and
/// a zero would claim they had and found nothing.
const V3: &str = "
ALTER TABLE rounds ADD COLUMN mode TEXT NOT NULL DEFAULT 'full';
ALTER TABLE rounds ADD COLUMN capped INTEGER NOT NULL DEFAULT 0;
ALTER TABLE rounds ADD COLUMN load_down_ms REAL;
ALTER TABLE rounds ADD COLUMN load_up_ms REAL;
ALTER TABLE rounds ADD COLUMN ping_down_samples INTEGER;
ALTER TABLE rounds ADD COLUMN ping_up_samples INTEGER;
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
    if version < 3 {
        tx.execute_batch(V3)?;
    }
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// F1's migration, on a database that already holds a user's rounds.
    /// Opening it must add the new columns and leave every existing round
    /// readable as what it was — a full round. A migration that dropped or
    /// re-created `rounds` would pass a "column exists" check and still lose
    /// the history, so the assertion is on the row.
    #[test]
    fn a_v2_database_gains_the_new_columns_and_its_rounds_backfill_as_full() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(V1).unwrap();
        conn.execute_batch(V2).unwrap();
        conn.pragma_update(None, "user_version", 2).unwrap();
        conn.execute(
            "INSERT INTO rounds (started_at, down_bps, bytes_down, bytes_up)
             VALUES (1700000000, 194100000.0, 104857600, 19000000)",
            [],
        )
        .unwrap();

        migrate(&conn).unwrap();

        let (mode, capped, down_bps, load_down): (String, i64, f64, Option<f64>) = conn
            .query_row(
                "SELECT mode, capped, down_bps, load_down_ms FROM rounds",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();

        assert_eq!(mode, "full", "every round stored so far was a full round");
        assert_eq!(capped, 0);
        assert_eq!(down_bps, 194_100_000.0, "the measurement itself survived");
        assert_eq!(
            load_down, None,
            "nobody measured the load window then; a zero would claim they did"
        );
        let version: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn migrating_twice_is_a_no_op() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        let version: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
    }
}
