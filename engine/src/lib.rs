//! Alidade engine: measurement traits, the Cloudflare provider, probes,
//! the round runner and scheduler. No I/O beyond network and clock, no UI.
//!
//! Probes, the round runner and the scheduler land in later tasks of
//! `docs/superpowers/plans/2026-08-20-alidade-engine.md` per the design in
//! `docs/superpowers/specs/2026-08-18-continuous-speed-test-design.md`.

mod provider;

pub use provider::{CloudflareProvider, EndpointConfig, MockProvider, SpeedProvider, Throughput};

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("not implemented")]
    NotImplemented,
    #[error("http request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("server returned status {0}")]
    Status(u16),
}
