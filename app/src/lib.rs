//! Alidade desktop app — library target.
//!
//! The crate carries both a library and a binary. `main.rs` is the thin
//! entry point that boots iced; everything testable lives here, so
//! integration tests under `app/tests/` can reach it as `alidade_app::…`.
//! Without this target a test file cannot import anything from the app,
//! which is why pure helpers (chart scales, version comparison) would
//! otherwise have to hide in `#[cfg(test)]` modules and stay untestable
//! from the outside.

pub mod chart;
pub mod screens;
pub mod state;
pub mod theme;
pub mod ui;
pub mod update;
