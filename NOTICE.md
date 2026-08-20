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
