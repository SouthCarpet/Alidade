# Alidade UI acceptance

This note records current desktop behavior, the plan 120 p3 evidence paths, historical screenshots and reviews, a measured single test from Task 2, and verification limits.

## What each screen does

### Home

Home is the starting screen. A 32 px page title leads one metadata line with the actual round mode and UTC time. One tinted, ruled field holds four facts: Download, Upload, Idle ping, and Ping under load. Labels use normal tracking and numbers use the tabular face. Idle packet loss is plain supporting text outside that field. **Run single test** and **Start continuous** keep their actions. At start Home loads the latest stored round, even if it is older than the chart window. Empty history has a smaller heading and a reason, with no circle or card. A read failure shows its reason instead of claiming there are no rounds.

### Boot history (plan 120 p3)

The boot query reads the selected database from the Unix epoch to the current time, then takes the last row in timestamp and id order. This uses the existing store API and reads all matching rows. Stored rows are kept as stored rows; missing live-only details are never reconstructed. A live result replaces the stored result, including when saving that live result fails.

Additive surface inventory:

- `LatestRound::Live` holds the live outcome and its save error.
- `LatestRound::Stored` holds the exact stored row.
- `LatestRound::started_at()` gives the timestamp of either source.
- `LatestRound::capped()` gives the recorded budget-cap state within the app.
- `LatestRound::persist_error()` gives a live save error within the app.
- `App::last_round` now holds either source in the same Home state.
- `App::last_round_error` holds a boot read error, separate from chart errors.
- Boot rule: read beyond the chart window, order by timestamp then id, and keep an empty store empty.
- Home read-error strings: `Stored round unavailable: <reason>`; saved measurement values remain distinct from missing values.

### Continuous

Continuous has a 32 px title and one metadata line with its actual state and recent-round count. Read errors and export notices use that same line, with full details on hover. A notice adds Dismiss to the controls. This keeps alerts from taking table height. Pause, Resume, Export CSV and range buttons sit beside the head. The existing chart is unchanged. The compact head and shell gaps leave room for the newest-round table's header and at least one complete row; it fills the remaining height and has its own scroll gutter. History keeps its earlier fixed table height.

### Ping monitor

Ping monitor lists each enabled target in a row with current latency, average, minimum, maximum, jitter, loss, and a sparkline. Start begins dense probing. A target with no answer is drawn in the danger colour.

### History

History filters stored rounds by From and To dates. It shows a count of rounds in range and a table of those rounds. Export CSV writes the filtered window.

### Settings

Settings has a 32 px title and one metadata line with the real interval and enabled-target count. Its editable body and scroll gutter are unchanged. It holds the measurement interval, which metrics run, the targets table (game-server presets may carry an unverified badge), daily data budget, theme, Windows autostart, and notifications. Notifications use a master switch and three threshold rows. Windows notification toasts are rate-limited. The in-app removal toast works on every supported desktop platform.

## Home composition and motion (plan 120 p3)

The early matrix has 16 captures: Home full, Home empty, Settings and Continuous, in light and dark at 1280 x 800 and the 1008 x 640 minimum. They are in `A:/tmp/120-p3-home/before/`, with a manifest. The same matrix after the change is in `A:/tmp/120-p3-home/after/`. The composition record is `A:/tmp/120-p3-home/composition.md` (under 60 lines). Both gallery Composed screen references were inspected. No user reference or phone target was provided or invented.

Captures use a fixed 30-second wait, a normal window launch, and PrintWindow of the visible Alidade window matched to the launched PID and exact title. The source helper is `A:/tmp/120-p3-home/CaptureWindow.cs`. No Computer Use, global mouse or keyboard input, or new PowerShell script file is used. Each manifest records state, theme, requested and actual size, source head, binary and image hashes, launch arguments, PID, and capture method. Seed data is local and the app never presses Run by itself.

The activity state changes only position and alpha over the base token (120 ms), bounded by sequence_max (320 ms). It stays still after entry, stops drawing at round completion, and never encodes estimated progress. Its 2 px layout slot remains reserved. The frame subscription runs only during entry. Windows client-area animation preference is read at boot and each round start; a failed preference query and non-Windows builds use the instant path. Reduced motion has the same text and focus order. The running line is covered by state tests, without an interactive capture.

Missing summary reasons stay on one line, with whitespace normalized for display and clipping at the available width. Hover shows the complete original reason in a flat note after the 500 ms tooltip token. No numeric value or unit is substituted for a missing measurement. A stored speed without old load details says so. The summary is a four-element array; loss remains outside it. The old KPI renderer, tracking helper, gauge, and two gauge-only tests are removed; the measurement-formatting tests remain.

Current audit limits block a full visual PASS: the existing light Settings unverified badge has 4.2894:1 text contrast (SLOP-COLOR-05), and vendor-check reports drift in the shared base tokens. The current brief permits Settings head changes only, excludes vendor writes, and forbids palette changes. Evidence is `A:/tmp/120-p3-home/contrast.md`; these issues need controller work before promotion. The unverified pill is also DECOR-06 watch, and the Continuous chart/register skeleton remains LAYOUT-07 watch. No chart or diagram conformance is claimed here; T3.2 owns that step.

Additive surface inventory, in addition to the boot inventory above:

- `SummaryFact::label` names one of the four summary measures.
- `SummaryFact::text` keeps a measured value with its unit, or the complete reason.
- `SummaryFact::measured` selects numeric typography within the app.
- `summary_facts()` returns exactly four facts from either source.
- `ActivityFrame::offset` is horizontal displacement in pixels.
- `ActivityFrame::alpha` is the line opacity from zero to one.
- `ActivityLine::started` holds an active run's start time, or None.
- `ActivityLine::reduced` selects the instant path.
- `ActivityLine::new()` creates idle activity with an explicit motion preference.
- `ActivityLine::start()` starts entry at an injected time.
- `ActivityLine::stop()` removes the line at once.
- `ActivityLine::frame()` computes position and alpha without changing layout.
- `ActivityLine::is_animating()` bounds the frame subscription to entry.
- `App::activity` holds the Home activity state for single and scheduled rounds.
- `App::activity_at` holds the latest animation frame time.
- `Message::ActivityTick(Instant)` advances only that frame time.
- `ActivityStrip::palette` supplies the existing accent color.
- `ActivityStrip::frame` supplies optional line geometry; idle draws nothing.
- `screens::page_head()` supplies the shared title, metadata and type scale.
- `screens::adaptive_rounds_table()` supplies the Continuous table viewport.
- Table rule: the compact head and 16 px shell gaps leave at least 52 px for the table header, one row and their gap before well padding at the 1008 x 640 minimum; History keeps its earlier height.
- `Palette::summary_field()` supplies one tint, 1 px hairline and shared radius.
- `Palette::summary_ink()` supplies the tint's existing foreground token.
- `Palette::reason_note()` supplies the full reason's flat note, up to 576 px wide.
- `Palette::tooltip_delay()` supplies the 500 ms hover delay token.
- `Palette::activity_duration()` supplies the base duration, capped by sequence_max.
- `Palette::activity_offset()` supplies the entry offset token.
- `Palette::activity_progress()` supplies the shared easing and bounded progress.
- Host rule: read Windows client-area animation preference; unknown and non-Windows preferences show the final line at once.
- Layout rule: keep a 2 px activity slot and use the 16 px shell section gap.
- Home head strings: Link measurement, Local measurement history, Full round and Ping round with an actual UTC time.
- Home summary string group: Download, Upload, Idle ping and Ping under load; reasons have no unit.
- Home missing-detail strings: sample count not stored, load details not stored for this round, loss not stored for this round.
- Home empty string group: No rounds yet and the explicit no-measurements guidance.
- Home state string group: plain Measuring, Idle packet loss, the save-error reason and the daily-budget explanation.
- Settings head string group: Settings, Measurement interval and Targets enabled with real values.
- Continuous head string group: Continuous, the existing state text, a real recent-round count, History read failed with its reason, and existing notice text with Dismiss.

## Shared controls (plan 120 p2)

| State | Expected result |
| --- | --- |
| Header | Alidade name and mark; Settings last; active tab uses a fill and square marker; a quick Light/Dark switch. |
| Appearance | Light, Dark, System segments; same saved `theme` as the header switch; System follows the OS. |
| About closed | Last section of Settings; opening it uses the shared consent gate. |
| About labels | Show About when collapsed; Hide About when expanded. |
| About, checks off | Version, explicit off message, consent checkbox, disabled Check now; no request from start, opening About, or Check now. |
| About, checks on | At most one request in flight; result and UTC time of the last check in this session; failures are shown. |
| Remove | Bottom-right Undo toast names the target for 10 seconds. Undo or Ctrl+Z outside text inputs restores its index, all fields, and raw port text. A stale timer cannot close a newer undo. |
| Help | Footer Help or F1 opens a searchable panel with idle ping, ping under load, jitter, and shortcuts. No animation; same text with reduced motion. |
| Keyboard | Ctrl+, opens Settings. Escape clears a non-empty Help search first, then closes Help and returns focus to the previous control. With Help closed, Escape dismisses Undo without restoring a target. Ctrl+Z in a focused text input does not restore a target. Help has keyboard focus and opens with Enter or Space. Tab stays in help search; Settings and Undo shortcuts do not act through the panel. |
| Window | Default 1280 x 800, minimum 1008 x 640; the Settings body scrolls in its own gutter. |

This run follows the controller's capture-only limit of 2026-10-24: no Computer Use or global pointer/key input. The existing startup flags capture the header at the top of Settings and Appearance with the closed About heading at the end (`--scroll settings-notifications`). Captures use light and dark themes at 1280 x 800 and 1008 x 640. Files are `A:/tmp/120-p2-maker/shots/header-{theme}-{size}.png` and `settings-tail-{theme}-{size}.png`, where theme is light/dark and size is default/min.

Expanded About with checks off or on and a last result, Undo, Help, and focus-return images are skipped. No existing setting or startup flag opens these states, and no new capture path was added. For every theme/size pair the skipped names are `about-off-{theme}-{size}.png`, `about-on-{theme}-{size}.png`, `undo-{theme}-{size}.png`, `help-{theme}-{size}.png`, and `focus-return-{theme}-{size}.png`.

Unit evidence for these states is in `app/tests/support/t21.rs` (consent and last result), `t22.rs` (complete target restoration, expiry and Ctrl+Z), `t23.rs` (help, shortcuts, focus capture and panel state), and `screens/help.rs` (the focus operation restores the recorded control without widget IDs). These tests check the same state transitions for reduced motion because the toast and panel have no animation state. Logs are under `A:/tmp/120-p2-maker/`. The screenshots below are historical captures.

## Historical screenshots

These older captures are 1280 by 800 pixels. Light is on the left. Dark is on the right. The current p3 matrix is recorded above.

### Home

| Light | Dark |
| --- | --- |
| ![Home, light](screens/home-light.png) | ![Home, dark](screens/home-dark.png) |

### Continuous

| Light | Dark |
| --- | --- |
| ![Continuous, light](screens/continuous-light.png) | ![Continuous, dark](screens/continuous-dark.png) |

### Ping monitor

| Light | Dark |
| --- | --- |
| ![Ping monitor, light](screens/ping-light.png) | ![Ping monitor, dark](screens/ping-dark.png) |

### History

| Light | Dark |
| --- | --- |
| ![History, light](screens/history-light.png) | ![History, dark](screens/history-dark.png) |

### Settings

| Light | Dark |
| --- | --- |
| ![Settings, light](screens/settings-light.png) | ![Settings, dark](screens/settings-dark.png) |

### Settings, Notifications block

These two shots use `--scroll settings-notifications` so the Notifications block is in view.

| Light | Dark |
| --- | --- |
| ![Settings Notifications, light](screens/settings-notifications-light.png) | ![Settings Notifications, dark](screens/settings-notifications-dark.png) |

## Historical visual verdict

Verdict at that review (re-check after the fix wave, not the current p3 verdict):

> Verdict: PARTIAL for Kaliber conformance. The rendered app reads as the same hand as the gallery, but the missing danger-row sparkline and two remaining minor defects prevent a PASS.

### Open in that review

- **Important.** `ping-light.png`, `ping-dark.png`: the `game-server-eu` row has historical measurements but its Sparkline cell is blank.

### Accepted minor findings in that review

- **Minor.** All four Settings images: the page scrollbar overlaps the right edge of the Targets container and the **Add target** button.
- **Minor.** `home-light.png`, `home-dark.png`: the empty state still combines its title and guidance into one line.

### Closed in the fix wave (first-round findings)

- **Critical (closed).** Ping monitor had no enabled targets in the first shots. Re-check: both seeded targets are listed. `cloudflare-anycast` shows current `11.1` and a sparkline. `game-server-eu` is in the danger colour with current `no answer`.
- **Important (closed).** History Loss values were clipped. Re-check: the complete Loss header and every `0.00` value are visible. The scrollbar sits in its own gutter.
- **Important (closed).** Continuous showed three table rows instead of eight. Re-check: eight rounds and seven columns; the well is sized to those rows.
- **Important (closed).** Notification thresholds were below the fold. Re-check: the extra Notifications shots show the master checkbox and the three thresholds (`Ping above 150 ms`, `Download below 10 Mbit/s`, `Loss above 5 %`).
- **Important (superseded by Kaliber r4).** Cards and wells now use a flat fill and a 1 px hairline. The former top highlight and card shadow are removed in both themes.

The first-round verdict was also PARTIAL. The five Critical and Important items above were the reason. After the wave, only the blank danger-row sparkline remains Important.

## Measured single-test numbers (Task 2)

One GUI single test against `speed.cloudflare.com` on **2026-08-20**. The round timestamp in the store is `2026-08-20T21:22:02Z`.

| Metric | Value |
| --- | --- |
| Date | 2026-08-20 |
| Download | 195.94 Mbit/s |
| Upload | 14.98 Mbit/s |
| Ping idle | 22.2 ms (store idle ping 22.25 ms) |
| Ping under load | 21.7 ms |
| Jitter | not measured |
| Loss | 0.00% |

The CLI export of the same row is `2026-08-20T21:22:02Z,full,195.94,14.98,22.25,21.3,22.090909...,5,0,false,,10001.2773,10636.7028`. `ping_down_ms` 21.3 and `ping_up_ms` about 22.09 average to 21.7 ms on the tile.

These historical Home captures use the seeded demo database, not that Task 2 round. At the time of those captures, Home only showed a live result and was empty after a restart. Plan 120 p3 now loads the latest stored round at start.

## Not covered yet

- Per-phase aggregate reporting is not in the UI.
- Chart zoom is not implemented. Range chips and hover are the only scale controls.
- Localisation: the UI is English only.
- Tray and toasts are Windows-first. macOS and Linux tray are not implemented.
- Live tray, left-click restore, close-to-tray, and toast appearance were proven by unit tests, not by a desktop capture (Task 6: live tray and toast could not be observed because desktop capture failed).
- Theme: `--theme light` and `--theme dark` produce distinct palettes in these shots. Choosing a theme in Settings is saved. A live OS dark-mode toggle for the System mode was not exercised on a real display.
- Tab-focus is a 2 px input border, not the gallery 2 px ring plus 2 px offset. Buttons have no focus ring (iced 0.14.2 limit, Task 5).
- Seconds on the Continuous countdown can skip because the tick is 1200 ms (Task 4).
- Ping samples match settings targets by name only. A renamed target that keeps the same name can inherit old samples (fix-wave note).
- Game-server presets in Settings may be unverified community data.

## How the screenshots were made

1. `seed_demo` writes a scratch store (60 rounds over 24 h, plus one hour of per-minute ping samples for `cloudflare-anycast` and `game-server-eu`) and a matching `settings.toml` next to it. Default path used by the shoot script: `%TEMP%\alidade-shoot\demo.db`.
2. `app/scripts/shoot.ps1` builds the app, seeds that database, then launches `alidade-app` with `--screen`, `--theme`, `--window 1280x800`, `--db`, and `--settings`.
3. The script waits a fixed 20 seconds, then takes one client-area capture. Polling `GetClientRect` right after launch often leaves the window at 0x0 for the rest of that process. The fixed wait avoids that stall.
4. Settings Notifications shots add `--scroll settings-notifications` so the scrollable snaps to the end before capture.
5. Home in this historical set is empty because the earlier app only showed a live result. Current Home loads the latest seeded round at start.

Fix round 1 captures: `A:/tmp/120-p2-fix1/shots/settings-tail-{light,dark}-default.png`. Window capture uses the seeded demo, a fixed 20-second wait and PrintWindow of the launched app. Regression cases are in `app/tests/support/t23.rs`.
