//! Alidade store: SQLite schema, migrations, retention/downsample,
//! queries and CSV export.

mod export;
mod retention;
mod schema;

use std::path::Path;
use std::time::{Duration, SystemTime};

use alidade_engine::{LoadWindow, PingSample, PingStats, RoundKind, RoundResult};
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
    #[error("stored round mode `{0}` is not a round kind this crate knows")]
    UnknownRoundMode(String),
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
    /// Which cadence produced this round (`rounds.mode`). A `PingOnly` row
    /// has no throughput because none was asked for; a `Full` row with empty
    /// speeds was asked and failed or was skipped — `skipped_reason` says
    /// which.
    pub mode: RoundKind,
    /// NULL when the metric was not selected, the provider errored, or the
    /// phase's own measured window fell too short of its budget to trust
    /// (F1) — `skipped_reason` says which. `bytes_down`/`bytes_up` below are
    /// independent of this: a discarded rate still moved real bytes.
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
    /// How long the download load actually lasted, in ms — what
    /// `ping_down_ms` was measured against. NULL on rounds stored before the
    /// window was recorded.
    pub load_down_ms: Option<f64>,
    pub load_up_ms: Option<f64>,
    /// Ping samples taken inside that load. Fewer than
    /// `MIN_UNDER_LOAD_SAMPLES` means the matching `ping_*_ms` is NULL
    /// because there was not enough load to measure — not because nothing
    /// ran.
    pub ping_down_samples: Option<i64>,
    pub ping_up_samples: Option<i64>,
    /// A byte ceiling ended a throughput phase, so the speeds in this row are
    /// a truncated measurement.
    pub capped: bool,
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
                 started_at, mode, down_bps, up_bps,
                 ping_idle_ms, ping_down_ms, ping_up_ms,
                 jitter_ms, loss_pct,
                 jitter_idle_ms, jitter_down_ms, jitter_up_ms,
                 loss_idle_pct, loss_down_pct, loss_up_pct,
                 bytes_down, bytes_up,
                 load_down_ms, load_up_ms, ping_down_samples, ping_up_samples,
                 capped, skipped_reason
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                       ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
        )?;
        let id = stmt.insert(params![
            started_at,
            r.kind.as_str(),
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
            bytes_moved(r.down_load),
            bytes_moved(r.up_load),
            load_ms(r.down_load),
            load_ms(r.up_load),
            load_samples(r.down_load),
            load_samples(r.up_load),
            r.capped,
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
            "SELECT id, started_at, mode, down_bps, up_bps,
                    ping_idle_ms, ping_down_ms, ping_up_ms,
                    jitter_ms, loss_pct,
                    jitter_idle_ms, jitter_down_ms, jitter_up_ms,
                    loss_idle_pct, loss_down_pct, loss_up_pct,
                    bytes_down, bytes_up,
                    load_down_ms, load_up_ms, ping_down_samples, ping_up_samples,
                    capped, skipped_reason
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

    /// Raw `rtt_ms` values stored for one target, oldest first. `None` is a
    /// recorded loss (SQL NULL), not `0.0` — exposed so tests can assert a
    /// miss reached storage as the right kind of absent, and so attribution
    /// (which target a row belongs to) is checkable by more than a total
    /// count.
    pub fn ping_sample_rtts_for(&self, target: &str) -> Result<Vec<Option<f64>>, StoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT rtt_ms FROM ping_samples WHERE target = ?1 ORDER BY at ASC")?;
        let rows = stmt.query_map(params![target], |row| row.get(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
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

/// Bytes a phase actually moved, read from its `LoadWindow` rather than from
/// `down`/`up`: a partial reading that got discarded to `None` (F1) still
/// moved real bytes, and this must not zero them out along with the rate
/// that was not trusted. `None` (the phase never ran) is 0 bytes.
fn bytes_moved(window: Option<LoadWindow>) -> i64 {
    window.map(|w| w.bytes as i64).unwrap_or(0)
}

/// How long the load actually lasted, in ms. NULL when the phase did not run
/// — the same distinction the `ping_*_ms` columns need to stay readable.
fn load_ms(window: Option<LoadWindow>) -> Option<f64> {
    window.map(|w| w.duration.as_secs_f64() * 1000.0)
}

fn load_samples(window: Option<LoadWindow>) -> Option<i64> {
    window.map(|w| i64::from(w.ping_samples))
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
    let mode_text: String = row.get(2)?;
    // An unrecognised mode is corrupt data. Defaulting it to `full` would
    // silently claim a throughput measurement that may never have happened.
    let mode = RoundKind::from_name(&mode_text).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            Box::new(StoreError::UnknownRoundMode(mode_text.clone())),
        )
    })?;
    Ok(RoundRow {
        id: row.get(0)?,
        started_at,
        mode,
        down_bps: row.get(3)?,
        up_bps: row.get(4)?,
        ping_idle_ms: row.get(5)?,
        ping_down_ms: row.get(6)?,
        ping_up_ms: row.get(7)?,
        jitter_ms: row.get(8)?,
        loss_pct: row.get(9)?,
        jitter_idle_ms: row.get(10)?,
        jitter_down_ms: row.get(11)?,
        jitter_up_ms: row.get(12)?,
        loss_idle_pct: row.get(13)?,
        loss_down_pct: row.get(14)?,
        loss_up_pct: row.get(15)?,
        bytes_down: row.get(16)?,
        bytes_up: row.get(17)?,
        load_down_ms: row.get(18)?,
        load_up_ms: row.get(19)?,
        ping_down_samples: row.get(20)?,
        ping_up_samples: row.get(21)?,
        capped: row.get(22)?,
        skipped_reason: row.get(23)?,
    })
}
