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

use std::time::Duration;

pub use config::{Settings, TargetSpec};
pub use probe::{probe_once, stats, PingSample, PingStats, Probe};
pub use provider::{CloudflareProvider, EndpointConfig, MockProvider, SpeedProvider, Throughput};
pub use round::{run_round, MetricSelection, RoundConfig, RoundResult};
pub use schedule::{RoundPlan, Scheduler};

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
    /// Catch-all for a provider-reported failure that isn't an HTTP/status/
    /// timeout error (e.g. `MockProvider::failing()` in tests). The round
    /// runner (Task 4) prefixes this with `"provider: "` when recording a
    /// skip reason.
    #[error("{0}")]
    Other(String),
}
