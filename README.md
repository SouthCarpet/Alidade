# Alidade

Alidade measures a fixed internet link over time and keeps the record. Each full round records download, upload, ping, and ping under load: whether latency gets worse while the link is busy. It stores the results in SQLite and can export CSV evidence for an ISP dispute. `ping-monitor` is a denser mode for watching a game-server or other target's latency.

## Features

- Full measurement rounds with download, upload, idle ping, and ping under load.
- Scheduled full and ping-only rounds, plus a dense ping monitor.
- Local SQLite history and CSV export.
- Editable targets and speed-test endpoint URLs in a plain TOML settings file.
- No telemetry, accounts, API keys, or phone-home. The only outbound traffic is the configured measurement itself. An opt-in update check against GitHub Releases is planned and is not in the current source.

## Quick start

Download `alidade.exe` from [Releases](https://github.com/SouthCarpet/Alidade/releases), then run:

```powershell
.\alidade.exe single
```

The CLI also provides `continuous`, `ping-monitor`, `export`, and `probe-targets`. Run `alidade.exe --help` for the current command help.

## Data and settings

- Database: `%LOCALAPPDATA%\Alidade\alidade.db`
- Settings: `%APPDATA%\Alidade\settings.toml`

Settings are plain, user-editable TOML. The default throughput source is Cloudflare's public `speed.cloudflare.com` `__down` and `__up` endpoints, behind a provider trait so another source can be added.

See [INSTALL.md](INSTALL.md), [CHANGELOG.md](CHANGELOG.md), and [KNOWN_ISSUES.md](KNOWN_ISSUES.md).

## Licence

Licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.

Alidade is independent and is not part of, affiliated with, endorsed by, or connected to Alibaba Group or any other third party; any similarity of name is coincidental.
