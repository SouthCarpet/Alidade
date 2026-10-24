# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- A latest-round Home diagram with separate Mbit/s bars and ms marks, a text ping difference, and an honest missing-round reason. New results draw in over 220 ms; reduced motion shows the final marks at once.

- Page heads on Home, Settings, and Continuous, with one metadata line per screen.
- One Home summary with four measured facts, missing-value reasons, and full reasons on hover.
- A Home activity line that enters by position and alpha, then stays still while the round runs. Reduced motion shows its final state at once.

- Load the latest stored round on Home at start, including rounds older than the chart window. Show a store read failure with its reason.

- Searchable Help on every screen and F1, with ping and jitter hints. Ctrl+, opens Settings. Escape clears Help search before closing Help and returning focus. Help and Undo show at once, including with reduced motion.

- A 10-second Undo toast for target removal, with Ctrl+Z support and full restoration of the target and its port input.

- Header with the Alidade mark and name, a theme quick switch, and a Light/Dark/System segmented control in Settings.
- About as the last Settings section: version, opt-in update checks at start, on opening About, and on Check now, with the last result and time for this session.

### Changed

- Refresh the vendored Kaliber dark neutrals through the vendor sync script.
- Replace the Settings unverified pill with plain AA-contrast state text.
- Separate Continuous throughput, latency and loss plots so each unit has its own scale. Remove the synthetic download range.
- Keep fractional chart tick labels distinct on scales with a span below four units.

- Replace the five Home KPI tiles and the empty-state circle with compact data and text guidance. Show idle packet loss below the summary.
- Replace the Continuous status pill with plain metadata. Keep a complete table row visible at minimum height and use the remaining space for scrolling.
- Use the 16 px shell section gap and tabular numbers in page metadata.

- Escape clears Help search before closing Help, or dismisses Undo when Help is closed. Ctrl+Z leaves target removal unchanged while a text input has focus.
- About uses the actions Show About and Hide About.

- Mark the active tab with a fill and a small square instead of an edge underline.

- Set the desktop minimum window size to 1008 x 640. The default remains 1280 x 800.
- Show all update-check text in English. Checks remain off by default and every trigger uses the same consent gate.

- Use Fira Sans for text and Fira Sans Tabular for metric numbers and chart labels, with a plain zero.

- Refresh design tokens to 0.10.0. Raised panes and wells use flat fills and 1 px hairlines. Remove the top highlight strips and card shadows.

- Settings: the page scrollbar has its own gutter, so it no longer covers the targets table or "Add target".

- Size the chart side margins from the widest axis label, so a label of three or four digits shows in full.

- Set the Rust cognitive complexity limit to 12 (`clippy.toml`). Three store tests and the startup flag parser were split into helpers to meet it. No assertion or flag behaviour changed.

## [0.1.0] - 2026-08-30

### Added

- Windows installer (per-user, no admin prompt) with desktop and Start Menu shortcuts; the uninstaller offers to delete measured data or keep it.
- Opt-in update check: Settings can ask GitHub once at start and show a link when a newer release exists. Nothing is downloaded.
- Measurement engine for time-bounded download and upload tests, idle ping, and ping under load.
- Configurable Cloudflare speed-test endpoints behind a provider trait.
- SQLite storage, retention/downsampling for ping-monitor samples, and CSV export.
- CLI commands for one-off measurements, scheduled measurements, ping monitoring, export, and target probing.
- Desktop UI with Home, Continuous, Ping monitor, History, and Settings screens.
- Windows system tray with close-to-tray behaviour.
- Windows toast notifications for configured threshold breaches.

### Fixed

- Copy diagnostics now labels its summary range as UTC, matching the CSV export, and four status strings no longer use em dashes.
