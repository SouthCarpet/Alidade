# Task 6 report — config, scheduler, and CLI harness

## Delivered

- Added `Settings` with user-editable TOML load/save, a 10-minute default,
  1-minute minimum validation, endpoint/metric/budget/retention settings, and
  data-driven targets. The LoL EUNE and Genshin EU presets are explicitly
  `verified = false` and retain their research-note provenance in saved TOML.
- Added a start-anchored `Scheduler`: overdue intervals yield one immediate
  run, never a catch-up burst; an exhausted daily budget disables throughput
  only and leaves ping enabled.
- Added the `single`, `continuous`, `ping`, `export`, and `probe-targets` CLI
  commands. `ALIDADE_DATA_DIR` is an optional test/sandbox override; the
  production default remains `%LOCALAPPDATA%\\Alidade\\alidade.db`.

## TDD evidence

RED:

```text
cargo test -p alidade-engine --test schedule
error[E0432]: unresolved imports `alidade_engine::Scheduler`,
`alidade_engine::Settings`
```

GREEN:

```text
cargo test --offline -p alidade-engine --test schedule
running 3 tests
... 3 passed; 0 failed
```

Final verification:

```text
cargo test --workspace
26 passed; 0 failed

cargo clippy --workspace --all-targets -- -D warnings
Finished `dev` profile ...
```

An initial dependency-resolution attempt hit Windows Schannel
`SEC_E_NO_CREDENTIALS`; after resolving from the local cache, the exact required
workspace commands above passed normally.

## Acceptance harness

```text
cargo run --offline -p alidade-cli -- single
download: skipped; upload: 7.72 Mbit/s; idle ping: 24.2 ms
skip reason: provider: server returned status 403
```

The round was persisted. Cloudflare rejected this environment's download
request with HTTP 403; upload still measured successfully and the failure was
recorded as data rather than fabricated as zero.

```text
cargo run --offline -p alidade-cli -- probe-targets
Cloudflare DNS  icmp  1.1.1.1                         yes  55.0 ms
Google DNS      icmp  8.8.8.8                         yes  58.0 ms
LoL EUNE        tcp   104.160.142.3:443               no   -
Genshin EU      tcp   hk4e-api-os.hoyoverse.com:443   yes  77.6 ms
```

```text
cargo run --offline -p alidade-cli -- export --from 1970-01-01 --to 2100-01-01 --out rounds.csv
started_at,down_mbps,up_mbps,ping_idle_ms,ping_down_ms,ping_up_ms,jitter_ms,loss_pct
```

The generated `rounds.csv` was inspected and removed so it does not become an
untracked source artifact.

## Files changed

- `engine/Cargo.toml` (required direct `tokio-util` dependency for the public
  `CancellationToken` scheduler API)
- `engine/src/config.rs`
- `engine/src/schedule.rs`
- `engine/src/lib.rs`
- `engine/tests/schedule.rs`
- `cli/Cargo.toml`
- `cli/src/main.rs`
- `Cargo.lock`
- `docs/task-6-report.md`

## Self-review and deviations

- No `unwrap`/`expect` was introduced in library code. Scheduler tests use no
  real-minute sleeps and no network.
- The brief's hard file-scope rule conflicts with its explicit request for
  this report and its request to amend the external research note. This report
  is present because it is explicitly required; the research note was not
  altered because doing so would violate the hard scope rule.
- `clap` derive could not be installed: its uncached derive crate required a
  registry download, and the sandbox's Schannel credentials rejected all
  requests. The CLI therefore uses a small standard-library parser with the
  exact requested command/flag surface, allowing the whole workspace to build,
  test, lint, and run offline.
- No commit was attempted, as directed.
