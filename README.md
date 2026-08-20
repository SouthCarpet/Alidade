# Alidade

Continuous internet speed test and link monitor. Runs time-aligned
measurement rounds (download, upload, ping, ping-under-load) against
Cloudflare and configurable probe targets, stores every round locally,
and exports history to CSV. Headless engine + store first, fully
testable before any UI exists; the UI (part 2) uses the Kaliber design
language, accent hue 190.

Local-only: no telemetry, no cloud, no API keys. The only network
destinations are the configured speed endpoints and the configured
probe targets.

## Database

`%LOCALAPPDATA%\Alidade\alidade.db` (SQLite). Rounds are kept forever.
Ping-monitor samples are kept raw for a configurable window (default 30
days), then downsampled to 1-minute aggregates.

## Workspace

- `engine/` (`alidade-engine`) — measurement traits, the Cloudflare
  provider, probes, the round runner, the scheduler. No I/O beyond
  network and clock, no UI.
- `store/` (`alidade-store`) — SQLite schema, migrations,
  retention/downsample, queries, CSV export.
- `cli/` (`alidade-cli`, binary `alidade`) — drives one round or a
  continuous loop from the terminal. The acceptance harness for the
  engine, and stays in the repo afterward as an ops tool.

## CLI (planned)

```
alidade single           # run one measurement round
alidade continuous       # run rounds on a schedule
alidade ping             # ping-only monitor loop
alidade export           # CSV export of a time range
alidade probe-targets    # list/check configured probe targets
```

All commands are stubs as of this scaffold (plan 052, task 1). The
build sequence lives in the parent projects-vault ecosystem at
`docs/superpowers/plans/2026-08-20-alidade-engine.md`, design decisions
in `docs/superpowers/specs/2026-08-18-continuous-speed-test-design.md`.
