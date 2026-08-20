//! Alidade engine: measurement traits, the Cloudflare provider, probes,
//! the round runner and scheduler. No I/O beyond network and clock, no UI.
//!
//! Stub crate (plan 052 / task 1, workspace scaffold). Real content lands
//! in later tasks of `docs/superpowers/plans/2026-08-20-alidade-engine.md`
//! per the design in
//! `docs/superpowers/specs/2026-08-18-continuous-speed-test-design.md`.

#[derive(Debug)]
pub enum EngineError {
    NotImplemented,
}
