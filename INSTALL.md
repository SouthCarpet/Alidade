# Install Alidade

## Windows binary

1. Download the `.exe` you need from the [GitHub Releases page](https://github.com/SouthCarpet/Alidade/releases).
2. Put it in a folder you control and run it from PowerShell or Command Prompt.

There is no installer. The binaries are unsigned. Windows SmartScreen may show a warning because it cannot establish a publisher reputation for an unsigned file. If you downloaded the file from this project's Releases page and choose to run it, select **More info**, then **Run anyway**. Do not disable SmartScreen.

For a first measurement:

```powershell
.\alidade.exe single
```

Use `.\alidade.exe --help` for the available commands and options.

## Data locations

- SQLite database: `%LOCALAPPDATA%\Alidade\alidade.db`
- User-editable settings: `%APPDATA%\Alidade\settings.toml`

The settings file is plain TOML. It contains the configured speed endpoints and probe targets.

## Build from source

This checkout declares Rust edition 2021 and does not contain a `rust-toolchain.toml`; no minimum compiler version is declared or verified. Install a current stable Rust toolchain, then run from the repository root:

```powershell
cargo build --release
```

The release binaries are placed under `target\release`. The workspace build currently includes the engine, store, CLI, and desktop app crates.
