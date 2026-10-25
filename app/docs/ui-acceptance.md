# Alidade UI acceptance

This note records current desktop behavior, the plan 120 p3 evidence paths, historical screenshots and reviews, a measured single test from Task 2, and verification limits.

## Release 0.2.0 captures (2026-10-25)

The current README images are [Home with full data](screens/release-0.2.0-home-light.png) and [Continuous](screens/release-0.2.0-continuous-light.png). Both use the 0.2.0 app, the light theme, and a 1280 x 800 client area. `seed_demo` writes 60 rounds and a matching scratch `settings.toml`. The launched app receives both `--db` and `--settings`. After a fixed 30-second wait, client-area PrintWindow captures only that app window. No measurement or global input is used.

Capture arguments, process IDs, binary and image hashes, and the source head are recorded in `A:/tmp/120-p4-release/captures.json`. The screenshot catalog scan is `A:/tmp/120-p4-release/slop-scan.md`. The separate historical set below is retained as history.

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

Continuous has a 32 px title and one metadata line with its actual state and recent-round count. Read errors and export notices use that same line, with full details on hover. A notice adds Dismiss to the controls. This keeps alerts from taking table height. Pause, Resume, Export CSV and range buttons sit beside the head. Throughput (Mbit/s), latency and jitter (ms), and loss (%) have separate labelled plots and independently computed domains. Missing values break each series. The synthetic download band and the unlabelled cross-series average are removed. The compact head and shell gaps leave room for the newest-round table's header and at least one complete row; it fills the remaining height and has its own scroll gutter. History keeps its earlier fixed table height.

### Ping monitor

Ping monitor lists each enabled target in a row with current latency, average, minimum, maximum, jitter, loss, and a sparkline. Start begins dense probing. A target with no answer is drawn in the danger colour.

### Plan 120 p3 review fixes

Each Continuous series has a labelled line sample below its plot, with the same colour, width and dash pattern as its plotted line. Keys wrap within their plot width at the minimum window size. Two key rows fit inside the existing chart height, so table space is unchanged. Continuous and History share right-aligned numeric headers and values; Time stays left-aligned. Home latency scale ends use one decimal (`42.9 ms`), matching the summary; throughput keeps two decimals.

Additive surface inventory:

- `chart::SeriesSpec` holds a series label, panel, stroke and metric accessor.
- `chart::series_specs()` supplies the exact same stroke to the plot and its key sample.
- `chart::key_entries(palette, panel)` supplies the labels and sample strokes consumed by each panel's key renderer.
- Chart key labels: Download, Upload, Idle, Load, Jitter, Packet loss.
- `screens::RoundColumn` defines the seven shared table columns; `alignment()` gives the alignment used by headers and values, and the private `label()` gives header text.
- `screens::drawing::Scale::end_label()` formats ms with one decimal and Mbit/s with two decimals.
- `screens::round_cell()` builds the text widget for headers and values from `RoundColumn`; tests lay out those widgets with the headless text renderer and inspect paragraph alignment.
- `screens::drawing::scale_texts()` supplies the canvas text draw list, including the formatted scale end and the missing-values reason.
- Key layout rule: wrap entries within each plot and reserve two rows within the existing canvas height.
- Regression evidence: `A:/tmp/120-p3-fix1/red.log`, `green.log`; captures in `A:/tmp/120-p3-fix1/after/`.
- Fix round 2 tests inspect panel key entries, built header/value cells and canvas text. Mutation logs, gate logs and unchanged-layout captures are in `A:/tmp/120-p3-fix2/`.
- The two fix round 2 light captures match every app pixel from fix round 1. Full-window equality is limited by native Windows title-bar differences in rows 0–30; capture used PrintWindow and a fixed 30-second wait.
- Fix round 3 tests call the actual Continuous and diagram canvas `draw()` functions and render their returned geometry with the existing headless tiny-skia renderer. Independent expected frames check all six labelled key strokes, both scale ends and both missing-values reasons. These tests reach the drawing loops, beyond the draw-list helpers tested in round 2.
- The fix round 3 table test lays out the actual row builder with six distinct measurements and checks all seven values in column order, with Time left-aligned and every numeric value right-aligned. This catches a numeric call site that incorrectly passes `RoundColumn::Time`.
- Fix round 3 mutation, gate and capture evidence is in `A:/tmp/120-p3-fix3/`; the test contract is `test-contract.md` and the two light captures are in `after/`.
- Both fix round 3 light captures match every app pixel from fix round 2. Native Windows title-bar differences remain limited to rows 0–30; capture used PrintWindow and a fixed 30-second wait.

### History

History filters stored rounds by From and To dates. It shows a count of rounds in range and a table of those rounds. Export CSV writes the filtered window.

### Settings

Settings has a 32 px title and one metadata line with the real interval and enabled-target count. Its body keeps the scroll gutter. It holds the measurement interval, which metrics run, the targets table, daily data budget, theme, Windows autostart, and notifications. Community game-server presets show plain `unverified` state text in the normal text role, with a tooltip that explains its meaning. No pill or warning fill is used. The text is checked against the target well in both themes at 4.5:1 or higher. Notifications use a master switch and three threshold rows. Windows notification toasts are rate-limited. The in-app removal toast works on every supported desktop platform.

## Latest-round diagram and draw-in (T3.2)

The diagram sits on one compact sheet below the four-fact Home summary, with the shared raised fill and 1 px hairline. Download and upload are two bars on a zero-based Mbit/s scale. Idle and loaded ping are two marks on a separate zero-based ms scale. Each domain ends at that pair's largest real value. A measured zero stays zero; missing or invalid data creates no mark. All four values remain text in the summary. The signed difference is plain text below the scales, or a reason when either ping is missing. An absent round draws no geometry and gives one reason. A store read failure keeps its error reason, without claiming that no round was measured. No dimension line, bracket, arrow or callout stripe is used.

Stored rows show their final state at boot. A new `RoundFinished` result starts three related moves: download, upload, then the ping pair. Starts are 0, 50 and 100 ms, with 120 ms per move and 220 ms total. Tokens supply the timing, easing and 8 px offset. Only x position and alpha change; values, lengths, scale domains, text and focus order are final immediately. The Windows reduced-motion preference is read when the result arrives. Reduced motion returns final frames at once and requests no animation subscription.

Vendor refresh uses only `app/vendor/vendor-sync.ps1`; dark neutral lightness follows the current source tokens. No theme-port change was needed for those values.

After captures: `A:/tmp/120-p3-diagram/after/manifest.json`, Home full/empty, Settings and Continuous, both themes, 1280 x 800 and actual minimum 1008 x 640 client size. Capture is PrintWindow of only the launched app, with fixed waits and seeded scratch data. Catalog scan: `A:/tmp/120-p3-diagram/slop-scan.md`.

Additive surface inventory:

- `screens::drawing` registers the new root `drawing.rs` through a path attribute, keeping the library entry point unchanged.
- `Scale::unit` names Mbit/s or ms.
- `Scale::values` holds two optional measured values.
- `Scale::domain()` computes a zero-based domain from that pair only.
- `Scale::fraction()` maps a real zero to zero without inventing a span.
- `Diagram::speed` holds download and upload in Mbit/s.
- `Diagram::latency` holds idle and loaded ping in ms.
- `diagram()` returns data or the missing-round reason.
- `Diagram::from_round()` reads either live or stored measurements.
- `Diagram::difference_text()` formats the signed load-minus-idle change or a reason.
- `Diagram::primitives()` emits only measured bars and marks.
- `Primitive::Bar` holds a speed-series index and a final fraction.
- `Primitive::Mark` holds a ping-series index and a final fraction.
- `valid()` rejects non-finite and negative input without creating zeros.
- `mean_present()` combines available loaded-ping phases by the existing summary rule.
- `drawing::view()` keeps text and layout static while passing mark frames to the canvas.
- `DiagramCanvas::data` holds the current diagram.
- `DiagramCanvas::palette` holds the active token palette.
- `DiagramCanvas::frames` holds the three position/alpha pairs.
- `DiagramCanvas::draw()` draws the two labelled scale areas and the measured primitive list.
- `DiagramCanvas::scale()` draws a unit title, row labels and a plain baseline.
- `DiagramCanvas::mark()` paints final geometry with position/alpha only.
- `canvas_text()` uses the tabular face for diagram labels.
- `MeasurementDrawIn::started` holds the result-arrival instant.
- `MeasurementDrawIn::reduced` holds the OS preference read at result arrival.
- `MeasurementDrawIn::new()` starts the result sequence with injected time.
- `MeasurementDrawIn::delays()` returns the three token-based start offsets.
- `MeasurementDrawIn::duration()` returns the complete sequence length.
- `MeasurementDrawIn::frames()` returns three bounded position/alpha pairs.
- `MeasurementDrawIn::is_animating()` stops frame subscriptions at completion or at once with reduced motion.
- `Palette::measurement_delays()` supplies the shared sequence-step token.
- `Palette::measurement_sheet()` supplies the compact sheet material and hairline, using the shared 8 px padding step.
- `App::measurement_draw_in` holds only the latest live-result sequence; boot starts with none.
- Frame subscription rule: existing `ActivityTick` updates the same clock for activity and result motion. No new message is added.
- `unverified_status()` supplies the plain state label and normal text color.
- `unverified_text()` renders that state without a badge and preserves its explanation tooltip.
- `chart_domains()` keeps throughput, latency and loss inputs separate.
- `panel_bounds()` returns three disjoint plot areas at the existing chart height.
- `LineChart::plots()` computes per-plot margins from its own domain.
- `LineChart::draw_panel()` paints only series with the matching unit.
- `ChartState::panel` keeps hover on the selected plot's unit scale.
- Axis-label rule: spans below four units use two decimal places, with margins measured from those same labels; larger spans keep integer labels.
- Diagram string group: Throughput (Mbit/s), Latency (ms), Download, Upload, Idle ping, Under load; endpoint units, ping change or missing reason, and No values in this round for an entirely missing pair.
- Continuous string group: Throughput (Mbit/s), Latency (ms), Loss (%); Download / Upload, Idle / Load / Jitter, Packet loss.
- Settings string group: `unverified` retains its existing community-data explanation.
- Test helper `measurement_diagram::round()` supplies known live measurements, with no network or store dependency.
- Test helper `measurement_diagram::ping()` supplies real `PingStats` for that fixture.
- `latest_round_supplies_bars_marks_and_ping_difference_without_fake_values` checks the measured values and signed difference.
- `missing_round_has_a_reason_and_no_diagram` checks the absent-round result.
- `throughput_and_latency_use_independent_zero_based_domains_and_units` checks independent units and domains.
- `draw_list_contains_only_two_speed_bars_and_two_latency_marks` checks the complete measured primitive list.
- `missing_measurements_keep_gaps_and_explain_the_missing_difference` checks absent phases.
- `reduced_motion_shows_all_three_final_moves_at_once` checks instant final frames.
- `new_measurement_uses_three_moves_fifty_ms_apart_and_finishes_by_320_ms` checks the sequence starts and bound.
- `unverified_target_stays_plain_text_with_aa_contrast_in_both_themes` checks the label and AA contrast.
- `continuous_chart_never_maps_loss_percent_onto_the_latency_scale` checks the loss/latency boundary and disjoint plots.
- `the_loss_axis_keeps_fractional_percent_ticks_distinct` checks readable quarter-percent tick labels.

## Home composition and motion (plan 120 p3)

The early matrix has 16 captures: Home full, Home empty, Settings and Continuous, in light and dark at 1280 x 800 and the 1008 x 640 minimum. They are in `A:/tmp/120-p3-home/before/`, with a manifest. The same matrix after the change is in `A:/tmp/120-p3-home/after/`. The composition record is `A:/tmp/120-p3-home/composition.md` (under 60 lines). Both gallery Composed screen references were inspected. No user reference or phone target was provided or invented.

Captures use a fixed 30-second wait, a normal window launch, and PrintWindow of the visible Alidade window matched to the launched PID and exact title. The source helper is `A:/tmp/120-p3-home/CaptureWindow.cs`. No Computer Use, global mouse or keyboard input, or new PowerShell script file is used. Each manifest records state, theme, requested and actual size, source head, binary and image hashes, launch arguments, PID, and capture method. Seed data is local and the app never presses Run by itself.

The activity state changes only position and alpha over the base token (120 ms), bounded by sequence_max (320 ms). It stays still after entry, stops drawing at round completion, and never encodes estimated progress. Its 2 px layout slot remains reserved. The frame subscription runs only during entry. Windows client-area animation preference is read at boot and each round start; a failed preference query and non-Windows builds use the instant path. Reduced motion has the same text and focus order. The running line is covered by state tests, without an interactive capture.

Missing summary reasons stay on one line, with whitespace normalized for display and clipping at the available width. Hover shows the complete original reason in a flat note after the 500 ms tooltip token. No numeric value or unit is substituted for a missing measurement. A stored speed without old load details says so. The summary is a four-element array; loss remains outside it. The old KPI renderer, tracking helper, gauge, and two gauge-only tests are removed; the measurement-formatting tests remain.

The T3.1 audit recorded a 4.2894:1 light Settings badge (SLOP-COLOR-05), vendor drift and a DECOR-06 pill watch. That task allowed only Settings head changes. T3.2 replaces the pill with plain text, refreshes the vendor tokens, and adds the diagram and separate Continuous plots described above. The chart/register skeleton remains LAYOUT-07 watch. T3.1 evidence stays in `A:/tmp/120-p3-home/contrast.md`; current after-image evidence is under `A:/tmp/120-p3-diagram/`.

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
