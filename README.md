# Alidade

Alidade measures a fixed internet link over time and keeps the record. Each full round records download, upload, ping, and ping under load: whether latency gets worse while the link is busy. It stores the results in SQLite and can export CSV evidence for an ISP dispute. `ping-monitor` is a denser mode for watching a game-server or other target's latency.

![Continuous screen](app/docs/screens/continuous-light.png)

## Features

- Home runs a single test.
- Continuous shows a chart and a table of rounds.
- Ping monitor shows target sparklines.
- History exports CSV and copies diagnostics.
- Settings edits targets and notification thresholds.
- The tray supports close-to-tray and toast notifications.

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

## Desktop appearance

The desktop app uses vendored Kaliber design tokens 0.10.0, with light and dark palettes. Raised panes use a tone step and a 1 px hairline. Text uses Fira Sans; metric numbers and chart labels use Fira Sans Tabular, with equal-width figures and a plain zero.

## Licence

Licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.

Alidade is an independent project. It is not affiliated with, endorsed by, or connected to any company, organisation, or product. An alidade is a sighting instrument used in surveying; any resemblance to another name is coincidental.
