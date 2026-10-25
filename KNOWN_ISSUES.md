# Known issues

These limits apply to Alidade **0.2.0**.

- The default **LoL EUNE** target (`104.160.142.3:443`) did not answer a live probe on 2026-08-20. Cloudflare DNS, Google DNS, and the Genshin EU host answered that probe. The preset is marked `verified = false` and can be edited or disabled in the settings file. Use `alidade.exe probe-targets` to check current reachability.
- The **Genshin EU** preset is also marked `verified = false`. It is user-editable; use `alidade.exe probe-targets` to check it from your own connection.
- A measurement round pings only the first enabled target. Use `ping-monitor` when you need dense monitoring of another configured target.
- The release binaries are unsigned, so Windows SmartScreen may warn on first run.
- A target whose recent samples are all losses can have a blank sparkline. The row is still marked as failing in the danger colour.
- The selected theme applies live. System follows the OS only when iced reports a theme change.
- Most buttons have no visible focus ring in this iced version. The Help opener has a 2 px focus ring and supports Enter and Space. Text inputs show a 2 px border.
- Home shows its empty state when the selected store has no rounds. It loads the latest stored round at start; a read failure shows its reason.
- Windows tray and notification toast behaviour was verified by tests, not observed live. In-app Undo is covered by state tests. Its live-input check is skipped under the capture-only limit.
- The tray and ICMP support are Windows-only.
- The UI is English only.
- Chart zoom is not implemented. Use range chips and hover.
- The design-tokens crate is vendored. `app/vendor/crates/design-tokens` is a copy of `design/crates/design-tokens` in the ecosystem. Closed 2026-09-20 (plan 094 item I): `design/scripts/test_vendor.py` compares every vendored file byte for byte against its source (the vendored `Cargo.toml`'s one addition, `vendor-sync.ps1`'s own marker block, is checked with a prefix match) and fails on drift, so a stale copy is caught by a gate instead of staying a silent risk. Refresh with `app/vendor/vendor-sync.ps1`; the crate's own `version` in `Cargo.toml` (0.10.0) names which schema the vendored copy carries.
