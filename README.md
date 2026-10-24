# Alidade

Alidade measures a fixed internet link over time and keeps the record. Each full round records download, upload, ping, and ping under load: whether latency gets worse while the link is busy. It stores the results in SQLite and can export CSV evidence for an ISP dispute. `ping-monitor` is a denser mode for watching a game-server or other target's latency.

![Continuous screen](app/docs/screens/continuous-light.png)

## Features

- Home runs a single test.
- Continuous shows a chart and a table of rounds.
- Ping monitor shows target sparklines.
- History exports CSV and copies diagnostics.
- Settings edits targets and notification thresholds, theme, and update consent. About is its last section.
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

## Desktop controls

The header shows the Alidade mark and name. Settings is the last tab. The header theme switch chooses Light or Dark from the theme in use. Settings > Appearance has Light, Dark, and System segments. System is the default and follows Windows. Both controls save the same `theme` preference in `ui-settings.toml`, beside `settings.toml`.

Settings > About shows the app version and the time and result of the last update check in this session. Update checks are off by default. When enabled, one request goes to GitHub at start, when About opens, and on **Check now**. The checkbox itself sends no request. With checks off, all three paths send no request and **Check now** is disabled. Alidade does not download or install an update.

The desktop window starts at 1280 × 800 and has a minimum size of 1008 × 640.

Removing a target shows a toast for 10 seconds. **Undo** or **Ctrl+Z** restores the complete target at the same position, including any unfinished port text. A later removal replaces the earlier undo. After the toast expires, the removal cannot be undone.

## Licence

Licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.

Alidade is an independent project. It is not affiliated with, endorsed by, or connected to any company, organisation, or product. An alidade is a sighting instrument used in surveying; any resemblance to another name is coincidental.
