# Vendored design files

The app builds from this public repository without the private design tree.
`crates/design-tokens` carries version 0.10.0. Tokens come from `tokens/`.

## Refresh

Run `pwsh app/vendor/vendor-sync.ps1` on the machine with the source at
`A:/projects-vault/design`. Then run the same command with `-Check`.
The explicit source and destination pairs match the Alidade vendor list in
`design/consumers.json`. The list includes the iced styles, motion and keys
modules, the contrast helper, the font helper and its font assets.
Only the Cargo.toml marker block is appended to the source copy.
It retains the existing package lint settings for generated output.

## Bundled fonts

Fira Sans and Fira Sans Tabular each have Regular, Medium, SemiBold and Bold
files under `assets/fonts/fira-sans/`, with their OFL licence.
The list also retains Inter and Atkinson assets under `fonts/`.
These assets stay in the vendor mapping; they need not be loaded by the app.

## Theme API changes from 0.2.0

The imported crate adds these palette roles. The port uses shared card and
well styles and the `control` boundary. It adds no new app setting.

- `surface_active`: pressed surface tone.
- `border_strong`: strong table or dialog rule.
- `control`: input and secondary button boundary.
- `disabled_text`: disabled text colour.
- `ink`: dark block fill.
- `on_ink`: text on ink.
- `on_ink_muted`: muted text on ink.
- `ink_rule`: rule on ink.
- `ink_action`: action fill on ink.
- `on_ink_action`: action label on ink.
- `surface_selected`: selected row fill. The current app has no selectable table row.
- `border_top`: removed; cards have no highlight strip.
- `milled`: removed; cards have no inner highlight.
- `Accent2.field`: optional second-hue section fill.
- `Accent2.highlight`: optional second-hue cell fill.
- `Accent2.mark`: optional second-hue comparison or category mark.
- `cognitive-complexity-threshold`: the app root sets a limit of 12 for Clippy.
