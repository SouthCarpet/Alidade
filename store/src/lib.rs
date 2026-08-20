//! Alidade store: SQLite schema, migrations, retention/downsample,
//! queries and CSV export.

mod export;
mod retention;
mod schema;

use std::path::Path;
use std::time::{Duration, SystemTime};

use alidade_engine::{PingSample, PingStats, RoundResult, Throughput};
use rusqlite::{params, Connection, Row};

/// Errors from opening, migrating, querying, or exporting the store.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("csv error: {0}")]
    Csv(#[from] csv::Error),
    #[error("timestamp is before the unix epoch or out of range")]
    InvalidTime,
    #[error("database schema version {0} is newer than this crate supports")]
    UnsupportedSchema(i32),
}

/// One persisted round, as stored (speeds in bits/s, times in ms).
///
/// `jitter_ms` / `loss_pct` are idle-baseline aliases (same values as
/// `jitter_idle_ms` / `loss_idle_pct`) kept for CSV compatibility. The
/// six per-phase columns are the bufferbloat signal.
#[derive(Debug, Clone)]
pub struct RoundRow {
    pub id: i64,
    pub started_at: SystemTime,
    pub down_bps: Option<f64>,
    pub up_bps: Option<f64>,
    pub ping_idle_ms: Option<f64>,
    pub ping_down_ms: Option<f64>,
    pub ping_up_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    pub loss_pct: Option<f64>,
    pub jitter_idle_ms: Option<f64>,
    pub jitter_down_ms: Option<f64>,
    pub jitter_up_ms: Option<f64>,
    pub loss_idle_pct: Option<f64>,
    pub loss_down_pct: Option<f64>,
    pub loss_up_pct: Option<f64>,
    pub bytes_down: i64,
    pub bytes_up: i64,
    pub skipped_reason: Option<String>,
}

/// avg/min/max over a single numeric metric, ignoring SQL NULLs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MetricAgg {
    pub avg: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

/// One persisted 1-minute ping aggregate.
#[derive(Debug, Clone, PartialEq)]
pub struct PingMinuteRow {
    pub target: String,
    pub minute: i64,
    pub avg_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub loss_pct: f64,
}

/// avg/min/max per round metric over a time range.
#[derive(Debug, Clone, PartialEq)]
pub struct Aggregates {
    pub down_bps: MetricAgg,
    pub up_bps: MetricAgg,
    pub ping_idle_ms: MetricAgg,
    pub ping_down_ms: MetricAgg,
    pub ping_up_ms: MetricAgg,
    pub jitter_ms: MetricAgg,
    pub loss_pct: MetricAgg,
}

/// SQLite-backed store for rounds and ping-monitor samples.
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open (or create) the database at `path` and run pending migrations.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        schema::migrate(&conn)?;
        Ok(Self { conn })
    }

    /// Persist one round. Returns the new `rounds.id`.
    pub fn insert_round(&self, r: &RoundResult) -> Result<i64, StoreError> {
        let started_at = unix_secs(r.started_at)?;
        let mut stmt = self.conn.prepare(
            "INSERT INTO rounds (
                 started_at, down_bps, up_bps,
                 ping_idle_ms, ping_down_ms, ping_up_ms,
                 jitter_ms, loss_pct,
                 jitter_idle_ms, jitter_down_ms, jitter_up_ms,
                 loss_idle_pct, loss_down_pct, loss_up_pct,
                 bytes_down, bytes_up, skipped_reason
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        )?;
        let id = stmt.insert(params![
            started_at,
            r.down.map(|t| t.bits_per_sec),
            r.up.map(|t| t.bits_per_sec),
            ping_avg(r.ping_idle),
            ping_avg(r.ping_down),
            ping_avg(r.ping_up),
            ping_jitter(r.ping_idle),
            r.ping_idle.map(|p| p.loss_pct),
            ping_jitter(r.ping_idle),
            ping_jitter(r.ping_down),
            ping_jitter(r.ping_up),
            r.ping_idle.map(|p| p.loss_pct),
            r.ping_down.map(|p| p.loss_pct),
            r.ping_up.map(|p| p.loss_pct),
            bytes_or_zero(r.down),
            bytes_or_zero(r.up),
            r.skipped_reason.as_deref(),
        ])?;
        Ok(id)
    }

    /// Persist raw ping-monitor samples for one target. Returns the number
    /// of rows inserted.
    pub fn insert_ping_samples(&self, target: &str, s: &[PingSample]) -> Result<usize, StoreError> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO ping_samples (target, at, rtt_ms) VALUES (?1, ?2, ?3)",
            )?;
            for sample in s {
                let at = unix_secs(sample.at)?;
                let rtt_ms = sample.rtt.map(|d| d.as_secs_f64() * 1000.0);
                stmt.execute(params![target, at, rtt_ms])?;
            }
        }
        tx.commit()?;
        Ok(s.len())
    }

    /// Rounds whose `started_at` falls in `[from, to]`, oldest first.
    pub fn rounds_between(
        &self,
        from: SystemTime,
        to: SystemTime,
    ) -> Result<Vec<RoundRow>, StoreError> {
        let from_secs = unix_secs(from)?;
        let to_secs = unix_secs(to)?;
        let mut stmt = self.conn.prepare(
            "SELECT id, started_at, down_bps, up_bps,
                    ping_idle_ms, ping_down_ms, ping_up_ms,
                    jitter_ms, loss_pct,
                    jitter_idle_ms, jitter_down_ms, jitter_up_ms,
                    loss_idle_pct, loss_down_pct, loss_up_pct,
                    bytes_down, bytes_up, skipped_reason
             FROM rounds
             WHERE started_at >= ?1 AND started_at <= ?2
             ORDER BY started_at ASC, id ASC",
        )?;
        let rows = stmt.query_map(params![from_secs, to_secs], row_to_round)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// avg/min/max of each round metric over `[from, to]`.
    pub fn round_aggregates(
        &self,
        from: SystemTime,
        to: SystemTime,
    ) -> Result<Aggregates, StoreError> {
        let from_secs = unix_secs(from)?;
        let to_secs = unix_secs(to)?;
        self.conn.query_row(
            "SELECT
                 AVG(down_bps), MIN(down_bps), MAX(down_bps),
                 AVG(up_bps), MIN(up_bps), MAX(up_bps),
                 AVG(ping_idle_ms), MIN(ping_idle_ms), MAX(ping_idle_ms),
                 AVG(ping_down_ms), MIN(ping_down_ms), MAX(ping_down_ms),
                 AVG(ping_up_ms), MIN(ping_up_ms), MAX(ping_up_ms),
                 AVG(jitter_ms), MIN(jitter_ms), MAX(jitter_ms),
                 AVG(loss_pct), MIN(loss_pct), MAX(loss_pct)
             FROM rounds
             WHERE started_at >= ?1 AND started_at <= ?2",
            params![from_secs, to_secs],
            |row| {
                Ok(Aggregates {
                    down_bps: metric_agg(row, 0)?,
                    up_bps: metric_agg(row, 3)?,
                    ping_idle_ms: metric_agg(row, 6)?,
                    ping_down_ms: metric_agg(row, 9)?,
                    ping_up_ms: metric_agg(row, 12)?,
                    jitter_ms: metric_agg(row, 15)?,
                    loss_pct: metric_agg(row, 18)?,
                })
            },
        )
        .map_err(StoreError::from)
    }

    /// Collapse raw ping samples older than `older_than_days` into
    /// `ping_minute` and delete those raw rows. Returns minute-rows written.
    pub fn downsample_pings(&self, older_than_days: u32) -> Result<usize, StoreError> {
        self.downsample_pings_at(SystemTime::now(), older_than_days)
    }

    /// Like [`Self::downsample_pings`], with an injected clock. Production
    /// callers use [`Self::downsample_pings`]; tests pin `now` so a cutoff
    /// can be placed inside a minute bucket.
    pub fn downsample_pings_at(
        &self,
        now: SystemTime,
        older_than_days: u32,
    ) -> Result<usize, StoreError> {
        retention::downsample_pings_at(&self.conn, now, older_than_days)
    }

    /// Minute-aggregate rows. Exposed so tests can assert downsample did not
    /// overwrite a whole-minute bucket with a later partial one.
    pub fn ping_minute_rows(&self) -> Result<Vec<PingMinuteRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT target, minute, avg_ms, min_ms, max_ms, loss_pct
             FROM ping_minute
             ORDER BY target ASC, minute ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(PingMinuteRow {
                target: row.get(0)?,
                minute: row.get(1)?,
                avg_ms: row.get(2)?,
                min_ms: row.get(3)?,
                max_ms: row.get(4)?,
                loss_pct: row.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Write rounds in `[from, to]` to `out` as CSV. Returns data-row count.
    pub fn export_rounds_csv(
        &self,
        from: SystemTime,
        to: SystemTime,
        out: &Path,
    ) -> Result<usize, StoreError> {
        let rows = self.rounds_between(from, to)?;
        export::write_rounds_csv(&rows, out)
    }

    /// Number of raw rows in `ping_samples`. Exposed so tests (and the CLI)
    /// can confirm retention actually deleted what it collapsed.
    pub fn ping_sample_count(&self) -> Result<usize, StoreError> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM ping_samples", [], |row| row.get(0))?;
        Ok(n as usize)
    }
}

pub(crate) fn unix_secs(t: SystemTime) -> Result<i64, StoreError> {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|_| StoreError::InvalidTime)
        .and_then(|d| i64::try_from(d.as_secs()).map_err(|_| StoreError::InvalidTime))
}

fn from_unix_secs(secs: i64) -> Result<SystemTime, StoreError> {
    let secs = u64::try_from(secs).map_err(|_| StoreError::InvalidTime)?;
    Ok(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
}

/// A phase with no RTT (nothing answered) stores SQL NULL, not `0.0`: a dead
/// target must not read back as a perfect one, and NULL keeps it out of the
/// `AVG`/`MIN`/`MAX` aggregates instead of dragging them down. The matching
/// `loss_*_pct` column still records that the phase happened and lost
/// everything.
fn ping_avg(stats: Option<PingStats>) -> Option<f64> {
    stats.and_then(|s| s.avg_ms)
}

fn ping_jitter(stats: Option<PingStats>) -> Option<f64> {
    stats.and_then(|s| s.jitter_ms)
}

fn bytes_or_zero(t: Option<Throughput>) -> i64 {
    t.map(|t| t.bytes as i64).unwrap_or(0)
}

fn metric_agg(row: &Row<'_>, start: usize) -> rusqlite::Result<MetricAgg> {
    Ok(MetricAgg {
        avg: row.get(start)?,
        min: row.get(start + 1)?,
        max: row.get(start + 2)?,
    })
}

fn row_to_round(row: &Row<'_>) -> rusqlite::Result<RoundRow> {
    let started_at_secs: i64 = row.get(1)?;
    let started_at = from_unix_secs(started_at_secs).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Integer,
            Box::new(e),
        )
    })?;
    Ok(RoundRow {
        id: row.get(0)?,
        started_at,
        down_bps: row.get(2)?,
        up_bps: row.get(3)?,
        ping_idle_ms: row.get(4)?,
        ping_down_ms: row.get(5)?,
        ping_up_ms: row.get(6)?,
        jitter_ms: row.get(7)?,
        loss_pct: row.get(8)?,
        jitter_idle_ms: row.get(9)?,
        jitter_down_ms: row.get(10)?,
        jitter_up_ms: row.get(11)?,
        loss_idle_pct: row.get(12)?,
        loss_down_pct: row.get(13)?,
        loss_up_pct: row.get(14)?,
        bytes_down: row.get(15)?,
        bytes_up: row.get(16)?,
        skipped_reason: row.get(17)?,
    })
}
