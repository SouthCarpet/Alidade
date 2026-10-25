# Install Alidade

These instructions apply to Alidade **0.2.0**.

## Windows installer

1. Download `alidade-setup-0.2.0.exe` from the [GitHub Releases page](https://github.com/SouthCarpet/Alidade/releases).
2. Run it. It installs for your user only, so there is no admin prompt.
3. It adds a desktop shortcut (you can turn this off in the wizard) and a Start Menu shortcut.
4. To remove Alidade, use **Settings > Apps** (or Programs and Features). The uninstaller asks whether to delete measured data (the database and settings) or keep it.

The installer is unsigned. Windows SmartScreen may show a warning because it cannot establish a publisher reputation for an unsigned file. If you downloaded the file from this project's Releases page and choose to run it, select **More info**, then **Run anyway**. Do not disable SmartScreen.

## Windows binary (portable)

1. Download `alidade-app.exe` for the desktop app or `alidade.exe` for the CLI from the [GitHub Releases page](https://github.com/SouthCarpet/Alidade/releases).
2. Put it in a folder you control and run it from PowerShell or Command Prompt.

There is no install step and nothing to remove. The same SmartScreen warning applies; see above.

For a first measurement:

```powershell
.\alidade.exe single
```

Use `.\alidade.exe --help` for the available commands and options.

## Record a checksum

To record the SHA-256 hash of a downloaded installer:

```powershell
Get-FileHash .\alidade-setup-0.2.0.exe -Algorithm SHA256
```

The release workflow does not publish a checksum manifest. This command records the hash of your local file; it does not compare it with a publisher checksum.

## Data locations

- SQLite database: `%LOCALAPPDATA%\Alidade\alidade.db`
- User-editable settings: `%APPDATA%\Alidade\settings.toml`

The settings file is plain TOML. It contains the configured speed endpoints and probe targets.

## Build from source

This checkout declares Rust edition 2021 and does not contain a `rust-toolchain.toml`; no minimum compiler version is declared or verified. Install a current stable Rust toolchain, then run from the repository root:

```powershell
cargo build --workspace --release
```

The release binaries are placed under `target\release`. The workspace build currently includes the engine, store, CLI, and desktop app crates.
