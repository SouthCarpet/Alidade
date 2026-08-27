# Alidade: Claude Code Context

Alidade measures a fixed internet link over time and keeps the record (download, upload, ping, ping under load) in local SQLite, with CSV export for ISP disputes. `ping-monitor` is a denser mode for watching a game server or other target's latency.

## Stack
- **Workspace**: Rust, cargo workspace with 4 members: `engine`, `store`, `cli`, `app`
- **CLI** (`alidade-cli`, binary `alidade`): clap 4.6, tokio 1.53, chrono 0.4
- **Desktop app** (`alidade-app`): iced 0.14 (canvas + tokio features), `design-tokens` crate (Kaliber design language, accent hue 190), reqwest 0.13 (rustls)
- **Engine** (`alidade-engine`): async-trait, reqwest 0.13 (rustls, stream), tokio-util; Windows ICMP via `windows` 0.62 IP Helper API (no admin rights), target-gated so non-Windows still compiles
- **Store** (`alidade-store`): rusqlite 0.40 (bundled), csv 1.3, chrono
- **License**: dual MIT OR Apache-2.0

## Build, test, run
```powershell
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings   # CI gate, zero warnings
cargo run -p alidade-cli -- single                       # one measurement round
cargo run -p alidade-cli -- continuous
cargo run -p alidade-cli -- ping-monitor
cargo run -p alidade-app                                 # iced desktop app
```
CI (`.github/workflows/ci.yml`) runs the same three cargo steps on `windows-latest` for pushes to `main`, tags, and PRs.

## Data and settings
- Database: `%LOCALAPPDATA%\Alidade\alidade.db`
- Settings: `%APPDATA%\Alidade\settings.toml` (plain, user-editable TOML; default throughput source is Cloudflare's public speed-test endpoints behind a provider trait)
- No telemetry, accounts, API keys, or phone-home traffic beyond the configured measurement itself.

## App-only rules
- Windows-first: the ICMP path is `cfg(windows)`-gated; keep new platform-specific code target-gated the same way rather than breaking the non-Windows build.
- Design surface (`app/`) follows the Kaliber design language (`vendor/crates/design-tokens`, accent hue 190). See `rule_design_language.md` in the root vault before changing colors or tokens.

## Pointers
- `.vault/`: per-app knowledge (Obsidian-indexable); `.vault/ops/cross-vaults.md` has `self_name: alidade`
- Registry name: `alidade` (`vault/ops/registry.yaml`)
- Repo: `A:/projects-vault/apps/alidade` (standalone git repo, `github.com/SouthCarpet/Alidade`)

<!-- multi-vault-block:v1 -->
### Multi-vault adapter (skills)

The central registry at `A:/projects-vault/vault/ops/registry.yaml` is the
single source of truth for which vaults exist on this machine. Any skill
that needs to enumerate or operate across vaults should call the central
adapter rather than re-parse the registry:

```powershell
py A:/projects-vault/scripts/multi_vault.py list --include-self --json
py A:/projects-vault/scripts/multi_vault.py paths --no-self
py A:/projects-vault/scripts/multi_vault.py current --json
py A:/projects-vault/scripts/multi_vault.py find <kw> --limit 10 --json
py A:/projects-vault/scripts/multi_vault.py for-each --cwd-mode project -- <cmd>
py A:/projects-vault/scripts/multi_vault.py doctor
```

The adapter auto-detects the caller's project from `$cwd`. No env var
required for the common case.

**Calling convention for upgraded skills:**

| Invocation                        | Scope                                         |
|-----------------------------------|-----------------------------------------------|
| `/<skill>`                        | THIS project's vault only (original behavior) |
| `/<skill> --all`                  | Every enabled vault in the registry           |
| `/<skill> --vault <name>`         | One named sibling vault                       |

No separate tag is required. Default behavior stays single-vault so
existing callers are unaffected. See
`A:/projects-vault/scripts/README.md` for the full list.
<!-- /multi-vault-block:v1 -->
