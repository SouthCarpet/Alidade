# Known issues

- The default **LoL EUNE** target (`104.160.142.3:443`) does not answer. A live probe on 2026-08-20 received no answer, while Cloudflare DNS, Google DNS, and the Genshin EU host answered. A reachable replacement today would be a Riot API edge, not a game server, so it would measure the path to Riot's front door rather than in-game latency. The preset is marked `verified = false` and can be edited or disabled in the settings file.
- The **Genshin EU** preset is also marked `verified = false`. It is user-editable; use `alidade.exe probe-targets` to check it from your own connection.
- A measurement round pings only the first enabled target. Use `ping-monitor` when you need dense monitoring of another configured target.
- The release binaries are unsigned, so Windows SmartScreen may warn on first run.
- A target whose recent samples are all losses can have a blank sparkline. The row is still marked as failing in the danger colour.
- The selected theme applies live. System follows the OS only when iced reports a theme change.
- Buttons have no visible focus ring in this iced version. Text inputs show a 2 px border.
- Home shows its empty state until a single test runs.
- Tray and toast behaviour was verified by tests, not observed live.
- The tray and ICMP support are Windows-only.
- The UI is English only.
- Chart zoom is not implemented. Use range chips and hover.
- The opt-in update check is not reachable. `update::check` in `app/src/update.rs` makes a single unauthenticated GET to the GitHub releases API, but it has no caller in the app or CLI. Nothing calls out, and no update prompt can appear. Wire it behind the Settings opt-in in a later task, or remove it.
- The design-tokens crate is vendored. `app/vendor/crates/design-tokens` is a copy of `design/crates/design-tokens` in the ecosystem. The token JSON under `app/vendor/tokens/` is byte-identical to `design/tokens/`, but the crate source differs in formatting and has no binary or tests. Upstream crate fixes do not reach the app until the vendored copy is refreshed. Add a drift check or use a path dependency.
