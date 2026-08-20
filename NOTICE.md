# Alidade notices

Alidade is an independent project. It is not affiliated with, endorsed by, or connected to any
company, organisation, or product. An alidade is a sighting instrument used in surveying; any
resemblance to another name is coincidental.

Copyright Michal

## Third-party dependencies

The direct dependencies below are taken from the workspace `Cargo.toml` files.
Their exact locked versions' manifest licence fields were checked in the local
Cargo registry.

| Dependency | Used by | Licence from manifest |
| --- | --- | --- |
| async-trait | engine | MIT OR Apache-2.0 |
| chrono | cli, store | MIT OR Apache-2.0 |
| clap | cli | MIT OR Apache-2.0 |
| csv | store | Unlicense/MIT |
| design-tokens (local path dependency) | app | Not declared |
| futures-core | engine | MIT OR Apache-2.0 |
| iced | app | MIT |
| reqwest | engine | MIT OR Apache-2.0 |
| rusqlite | store | MIT |
| thiserror | engine, store | MIT OR Apache-2.0 |
| tokio | cli, engine | MIT |
| tokio-util | engine | MIT |
| windows (Windows only) | engine | MIT OR Apache-2.0 |

The development-only dependencies are `tempfile` (MIT OR Apache-2.0) and
`wiremock` (MIT/Apache-2.0).

## Bundled fonts

`app/vendor/fonts/` carries the **Inter** typeface (Regular, Medium, SemiBold),
copyright the Inter Project Authors, licensed under the **SIL Open Font License,
Version 1.1**. The full licence text ships beside the fonts as
`app/vendor/fonts/OFL.txt`, as the OFL requires. Inter is redistributed
unmodified; it is not a Reserved Font Name usage.

## Vendored design tokens

`app/vendor/crates/design-tokens/` is a copy of an internal crate that turns the
token files in `app/vendor/tokens/` into typed Rust colour and scale constants.
It is vendored so this repository builds from a clone without any tree outside
it. `app/vendor/vendor-sync.ps1` refreshes the copy, and `-Check` reports drift
without changing anything. The copy is covered by this project's own
`MIT OR Apache-2.0` licence.
