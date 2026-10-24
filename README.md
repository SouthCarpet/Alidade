# Alidade

Alidade measures a fixed internet link over time and keeps the record. Each full round records download, upload, ping, and ping under load: whether latency gets worse while the link is busy. It stores the results in SQLite and can export CSV evidence for an ISP dispute. `ping-monitor` is a denser mode for watching a game-server or other target's latency.

## Features

- Home loads the latest stored round at start and runs a single test. An empty store shows the empty state; a read failure shows its reason.
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

Home reads stored history from the database selected by `--db`, or the normal app database. It keeps stored rows separate from live results, so it does not invent sample counts or durations that were not saved. The existing store query reads rounds from the Unix epoch to the current time before selecting the newest. This also loads a last round older than the chart window.

Home has a page title and one line with the round mode and UTC time. One tinted summary shows download, upload, idle ping, and ping under load. Idle packet loss is supporting text below it. Missing measurements show a reason instead of a zero; the full reason is available on hover. An empty store has text guidance and no placeholder illustration.

Below the summary, the latest round has two labelled scales: download and upload bars in Mbit/s, and idle and loaded ping marks in ms. Each scale starts at zero and ends at its own largest measured value. A measured zero stays zero; a missing value has no mark. The ping change is also printed as text. All four values remain in the summary. With no round, Home gives a one-line reason and draws no diagram. Speed and latency have no dimension lines or arrows.

A newly finished round moves and fades in its two speed bars and ping marks in three steps, 50 ms apart, for 220 ms in total. Geometry and text use the final measured values from the first frame. Reduced motion shows the final marks at once. Stored history does not animate at start.

While a round runs, Home shows **Measuring** and a 2 px activity line. The line enters by position and alpha over the 120 ms token, capped at 320 ms, then stays still until the round ends. Its length does not estimate completion. The line has no geometry while idle, and its layout space stays reserved. Windows animation preference is read at start and before each round. With reduced motion, an unreadable preference, or a non-Windows platform, the final line shows at once. Text and focus stay the same.

Settings and Continuous use the same page-title scale and one metadata line. Continuous uses plain state text. Read errors and export notices use that line, with full details on hover and a Dismiss action for notices. Its compact head leaves room for the table header and a complete row at the minimum window size; more rows remain available in the table's scroll area. Its chart keeps throughput (Mbit/s), latency and jitter (ms), and loss (%) in separate labelled plots. Missing values break a series. The chart draws measured values only, without a synthetic download range.

Settings shows an unverified community target as plain `unverified` text in the normal text role. It meets 4.5:1 contrast in both themes. Dark surfaces use the current, darker Kaliber neutral tokens.

Chart axes use two decimal places when their span is below four units, so fractional percent ticks stay distinct. Wider scales keep compact integer labels. Axis margins use the displayed labels.

The header shows the Alidade mark and name. Settings is the last tab. The header theme switch chooses Light or Dark from the theme in use. Settings > Appearance has Light, Dark, and System segments. System is the default and follows Windows. Both controls save the same `theme` preference in `ui-settings.toml`, beside `settings.toml`.

Settings > **Show About** expands the About section; **Hide About** collapses it. About shows the app version and the time and result of the last update check in this session. Update checks are off by default. When enabled, one request goes to GitHub at start, when About opens, and on **Check now**. The checkbox itself sends no request. With checks off, all three paths send no request and **Check now** is disabled. Alidade does not download or install an update.

The desktop window starts at 1280 x 800 and has a minimum size of 1008 x 640.

Removing a target shows a toast for 10 seconds. **Undo** or **Ctrl+Z** outside text inputs restores the complete target at the same position, including any unfinished port text. A later removal replaces the earlier undo. After the toast expires, the removal cannot be undone.

**Help (F1)** is at the foot of every screen. The panel has searchable explanations of idle ping, ping under load, jitter, and shortcuts. **Ctrl+,** opens Settings. **Escape** first clears a non-empty Help search. The next Escape closes Help and returns focus to the control that opened it. With Help closed, Escape dismisses the Undo toast without restoring the target. Tab can reach the Help opener; Enter or Space opens it. Tab stays in the help search while the panel is open. The panel and undo toast appear at once, with the same text when reduced motion is enabled.

## Licence

Licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.

Alidade is an independent project. It is not affiliated with, endorsed by, or connected to any company, organisation, or product. An alidade is a sighting instrument used in surveying; any resemblance to another name is coincidental.
