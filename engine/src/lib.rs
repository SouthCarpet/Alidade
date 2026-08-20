//! Alidade engine: measurement traits, the Cloudflare provider, probes,
//! the round runner and scheduler. No I/O beyond network and clock, no UI.
//!
//! The round runner and the scheduler land in later tasks of
//! `docs/superpowers/plans/2026-08-20-alidade-engine.md` per the design in
//! `docs/superpowers/specs/2026-08-18-continuous-speed-test-design.md`.

mod config;
mod probe;
mod provider;
mod round;
mod schedule;

use std::fmt;
use std::time::Duration;

pub use config::{Settings, TargetSpec, MIN_CADENCE};
pub use probe::{probe_once, stats, PingSample, PingStats, Probe};
pub use provider::{
    CloudflareProvider, EndpointConfig, MockProvider, SpeedProvider, Throughput,
    DOWNLOAD_CHUNK_BYTES, MAX_REQUEST_BYTES, UPLOAD_CHUNK_BYTES,
};
pub use round::{
    run_round, LoadWindow, MetricSelection, RoundConfig, RoundKind, RoundResult,
    MIN_UNDER_LOAD_SAMPLES,
};
pub use schedule::{RoundPlan, Scheduler};

/// A 429 from the throughput source. Distinct from [`EngineError::Status`]:
/// a rate limit is one condition that lasts across rounds, not a per-round
/// hiccup, and the round runner records this type's Display as
/// `skipped_reason` so the UI and CSV can say so.
///
/// `consecutive` is the number of 429s received since the last successful
/// download phase. `>= 2` is the sustained case (the limit was still there
/// when we came back). A skip while still inside the backoff window reuses
/// the same count — it is the same condition, not a new failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimit {
    pub retry_after: Duration,
    pub consecutive: u32,
}

impl fmt::Display for RateLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let wait = format_wait(self.retry_after);
        if self.consecutive >= 2 {
            write!(
                f,
                "throughput source is rate-limiting us (sustained, {} 429s); retry after {wait}",
                self.consecutive
            )
        } else {
            write!(
                f,
                "throughput source is rate-limiting us; retry after {wait}"
            )
        }
    }
}

fn format_wait(wait: Duration) -> String {
    // Sub-second remainders still have to read as a wait, not "0s".
    let secs = wait.as_secs().max(1);
    if secs.is_multiple_of(3600) {
        format!("{}h", secs / 3600)
    } else if secs.is_multiple_of(60) {
        format!("{}m", secs / 60)
    } else if secs >= 60 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("not implemented")]
    NotImplemented,
    #[error("http request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("server returned status {0}")]
    Status(u16),
    #[error("server did not respond within {0:?}")]
    Timeout(Duration),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid settings: {0}")]
    Config(String),
    /// See [`RateLimit`]. The round runner prefixes this with `"provider: "`
    /// when recording a skip reason, same as every other provider error.
    #[error("{0}")]
    RateLimited(RateLimit),
    /// Catch-all for a provider-reported failure that isn't an HTTP/status/
    /// timeout error (e.g. `MockProvider::failing()` in tests). The round
    /// runner (Task 4) prefixes this with `"provider: "` when recording a
    /// skip reason.
    #[error("{0}")]
    Other(String),
}
