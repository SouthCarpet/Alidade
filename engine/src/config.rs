//! User-editable settings and the shipped, explicitly unverified game presets.

use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::{EndpointConfig, EngineError, MetricSelection, Probe};

/// A probe target configured as data rather than embedded in measurement logic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetSpec {
    pub name: String,
    pub probe: Probe,
    pub verified: bool,
    pub enabled: bool,
}

/// Persistent application settings. Intervals are stored in TOML as whole seconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub endpoints: EndpointConfig,
    pub interval: Duration,
    pub metrics: MetricSelection,
    pub targets: Vec<TargetSpec>,
    pub daily_budget_bytes: Option<u64>,
    pub raw_ping_retention_days: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            endpoints: EndpointConfig::default(),
            interval: Duration::from_secs(10 * 60),
            metrics: MetricSelection {
                download: true,
                upload: true,
                ping: true,
            },
            targets: vec![
                TargetSpec {
                    name: "Cloudflare DNS".to_string(),
                    probe: Probe::Icmp {
                        host: "1.1.1.1".to_string(),
                    },
                    verified: true,
                    enabled: true,
                },
                TargetSpec {
                    name: "Google DNS".to_string(),
                    probe: Probe::Icmp {
                        host: "8.8.8.8".to_string(),
                    },
                    verified: true,
                    enabled: true,
                },
                // Unverified community data: raw_speedtest_targets_2026-08.md.
                TargetSpec {
                    name: "LoL EUNE".to_string(),
                    probe: Probe::TcpConnect {
                        host: "104.160.142.3".to_string(),
                        port: 443,
                    },
                    verified: false,
                    enabled: true,
                },
                // Unverified community data: raw_speedtest_targets_2026-08.md.
                TargetSpec {
                    name: "Genshin EU".to_string(),
                    probe: Probe::TcpConnect {
                        host: "hk4e-api-os.hoyoverse.com".to_string(),
                        port: 443,
                    },
                    verified: false,
                    enabled: true,
                },
            ],
            daily_budget_bytes: None,
            raw_ping_retention_days: 30,
        }
    }
}

impl Settings {
    /// Load the TOML settings file, creating it from the compiled defaults
    /// the first time. Returns the settings and whether *this* call wrote
    /// the file (so the caller can tell the user where it now lives).
    ///
    /// Writing on first use is the point: the presets — including the two
    /// marked `verified = false` and their provenance comments — are only
    /// "user-editable" once they are actually on disk. What comes back is
    /// re-read from that file, so a defaults-to-TOML-to-defaults mismatch
    /// surfaces immediately instead of silently diverging from what the user
    /// sees.
    ///
    /// A file that exists but does not parse is an error and is **never**
    /// overwritten: a broken hand edit must stay repairable, not be replaced
    /// by defaults behind the user's back.
    pub fn load_or_create(path: &Path) -> Result<(Self, bool), EngineError> {
        let created = !path.exists();
        if created {
            Self::default().save(path)?;
        }
        let content = fs::read_to_string(path)?;
        Ok((Self::parse(&content)?, created))
    }

    /// Default `%APPDATA%\\Alidade\\settings.toml` location (with a portable fallback).
    pub fn default_path() -> PathBuf {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("Alidade").join("settings.toml")
    }

    /// Save an editable TOML file. The two game presets retain their
    /// provenance comments.
    ///
    /// Atomic: the bytes go to a scratch file next to the destination, are
    /// flushed to the disk, and only then replace the destination by rename.
    /// Writing straight onto `settings.toml` truncates it first, so a crash
    /// or a full disk mid-write would leave the user with an empty or
    /// half-written settings file and no way back.
    pub fn save(&self, path: &Path) -> Result<(), EngineError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let scratch = scratch_path_for(path);
        let mut file = fs::File::create(&scratch)?;
        file.write_all(self.to_toml().as_bytes())?;
        // Without this the rename can land before the content does, which is
        // the same lost-settings outcome one power cut later.
        file.sync_all()?;
        drop(file);
        fs::rename(&scratch, path)?;
        Ok(())
    }

    /// Fast deterministic fixture used by scheduler tests.
    pub fn test_default(interval: Duration) -> Self {
        Self {
            interval,
            ..Self::default()
        }
    }

    fn parse(input: &str) -> Result<Self, EngineError> {
        let mut settings = Self::default();
        let mut current: Option<TargetDraft> = None;
        let mut section = "";
        let mut target_section_seen = false;

        for raw in input.lines() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if line == "[[targets]]" {
                if !target_section_seen {
                    settings.targets.clear();
                    target_section_seen = true;
                }
                if let Some(target) = current.take() {
                    settings.targets.push(target.finish()?);
                }
                current = Some(TargetDraft::default());
                section = "targets";
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                if let Some(target) = current.take() {
                    settings.targets.push(target.finish()?);
                }
                section = &line[1..line.len() - 1];
                continue;
            }
            let (key, value) = split_assignment(line)?;
            if section == "targets" {
                let target = current
                    .as_mut()
                    .ok_or_else(|| EngineError::Config("target outside [[targets]]".to_string()))?;
                target.set(key, value)?;
            } else {
                apply_top_level(&mut settings, section, key, value)?;
            }
        }
        if let Some(target) = current {
            settings.targets.push(target.finish()?);
        }
        if settings.interval < Duration::from_secs(60) {
            return Err(EngineError::Config(
                "interval must be at least 60 seconds".to_string(),
            ));
        }
        Ok(settings)
    }

    fn to_toml(&self) -> String {
        let mut out = format!(
            "interval_seconds = {}\n{}raw_ping_retention_days = {}\n\n[endpoints]\ndownload_url = \"{}\"\nupload_url = \"{}\"\n\n[metrics]\ndownload = {}\nupload = {}\nping = {}\n",
            self.interval.as_secs(), budget_lines(self.daily_budget_bytes), self.raw_ping_retention_days,
            escape(&self.endpoints.download_url), escape(&self.endpoints.upload_url),
            self.metrics.download, self.metrics.upload, self.metrics.ping,
        );
        for target in &self.targets {
            let (kind, host, port) = match &target.probe {
                Probe::Icmp { host } => ("icmp", host.as_str(), None),
                Probe::TcpConnect { host, port } => ("tcp", host.as_str(), Some(*port)),
            };
            if !target.verified {
                out.push_str("\n# Unverified community preset; edit or remove after probing.\n");
            }
            out.push_str(&format!(
                "\n[[targets]]\nname = \"{}\"\nkind = \"{}\"\nhost = \"{}\"\nverified = {}\nenabled = {}\n",
                escape(&target.name), kind, escape(host), target.verified, target.enabled,
            ));
            if let Some(port) = port {
                out.push_str(&format!("port = {}\n", port));
            }
        }
        out
    }
}

#[derive(Default)]
struct TargetDraft {
    name: Option<String>,
    kind: Option<String>,
    host: Option<String>,
    port: Option<u16>,
    verified: Option<bool>,
    enabled: Option<bool>,
}

impl TargetDraft {
    fn set(&mut self, key: &str, value: &str) -> Result<(), EngineError> {
        match key {
            "name" => self.name = Some(parse_string(value)?),
            "kind" => self.kind = Some(parse_string(value)?),
            "host" => self.host = Some(parse_string(value)?),
            "port" => self.port = Some(parse_number(value)?),
            "verified" => self.verified = Some(parse_bool(value)?),
            "enabled" => self.enabled = Some(parse_bool(value)?),
            _ => return Err(EngineError::Config(format!("unknown target key `{key}`"))),
        }
        Ok(())
    }

    fn finish(self) -> Result<TargetSpec, EngineError> {
        let name = required(self.name, "target name")?;
        let host = required(self.host, "target host")?;
        let kind = required(self.kind, "target kind")?;
        let probe = match kind.as_str() {
            "icmp" => Probe::Icmp { host },
            "tcp" => Probe::TcpConnect {
                host,
                port: required(self.port, "tcp target port")?,
            },
            _ => return Err(EngineError::Config(format!("unknown target kind `{kind}`"))),
        };
        Ok(TargetSpec {
            name,
            probe,
            verified: self.verified.unwrap_or(false),
            enabled: self.enabled.unwrap_or(true),
        })
    }
}

fn apply_top_level(
    settings: &mut Settings,
    section: &str,
    key: &str,
    value: &str,
) -> Result<(), EngineError> {
    match (section, key) {
        ("", "interval_seconds") => settings.interval = Duration::from_secs(parse_number(value)?),
        ("", "daily_budget_bytes") => settings.daily_budget_bytes = parse_optional_number(value)?,
        ("", "raw_ping_retention_days") => settings.raw_ping_retention_days = parse_number(value)?,
        ("endpoints", "download_url") => settings.endpoints.download_url = parse_string(value)?,
        ("endpoints", "upload_url") => settings.endpoints.upload_url = parse_string(value)?,
        ("metrics", "download") => settings.metrics.download = parse_bool(value)?,
        ("metrics", "upload") => settings.metrics.upload = parse_bool(value)?,
        ("metrics", "ping") => settings.metrics.ping = parse_bool(value)?,
        _ => {
            return Err(EngineError::Config(format!(
                "unknown settings key `{key}` in [{section}]"
            )))
        }
    }
    Ok(())
}

fn split_assignment(line: &str) -> Result<(&str, &str), EngineError> {
    line.split_once('=')
        .map(|(key, value)| (key.trim(), value.trim()))
        .ok_or_else(|| EngineError::Config(format!("expected key = value, got `{line}`")))
}

fn parse_string(value: &str) -> Result<String, EngineError> {
    let trimmed = value.trim();
    if trimmed.len() < 2 || !trimmed.starts_with('"') || !trimmed.ends_with('"') {
        return Err(EngineError::Config(format!(
            "expected quoted string, got `{value}`"
        )));
    }
    Ok(trimmed[1..trimmed.len() - 1]
        .replace("\\\"", "\"")
        .replace("\\\\", "\\"))
}

fn parse_bool(value: &str) -> Result<bool, EngineError> {
    match value.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(EngineError::Config(format!(
            "expected boolean, got `{value}`"
        ))),
    }
}

fn parse_number<T: std::str::FromStr>(value: &str) -> Result<T, EngineError> {
    value
        .trim()
        .parse()
        .map_err(|_| EngineError::Config(format!("expected number, got `{value}`")))
}

/// `none` is not a TOML literal; earlier versions of this file wrote it, so
/// it is still accepted on read. New files express "no budget" the valid
/// way — by leaving the key out (see `budget_lines`).
fn parse_optional_number(value: &str) -> Result<Option<u64>, EngineError> {
    if value.trim() == "none" {
        Ok(None)
    } else {
        parse_number(value).map(Some)
    }
}

fn required<T>(value: Option<T>, name: &str) -> Result<T, EngineError> {
    value.ok_or_else(|| EngineError::Config(format!("missing {name}")))
}

/// The `daily_budget_bytes` line, or — when there is no budget — a commented
/// example instead of a key. TOML has no `none` literal, and the file has to
/// survive being opened by a real TOML editor; an absent key says "off" in
/// valid TOML while the comment keeps the setting discoverable.
fn budget_lines(value: Option<u64>) -> String {
    match value {
        Some(bytes) => format!("daily_budget_bytes = {bytes}\n"),
        None => "# No daily transfer budget. Uncomment and set the bytes to enable one:\n# daily_budget_bytes = 5000000000\n".to_string(),
    }
}

/// Scratch file [`Settings::save`] writes before renaming it onto `path`. It
/// stays in the destination's own directory on purpose: a rename is only
/// atomic within one volume, so a scratch file in `%TEMP%` would quietly
/// degrade into a copy whenever the two sit on different drives.
fn scratch_path_for(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .unwrap_or_else(|| OsStr::new("settings.toml"))
        .to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir() -> tempfile::TempDir {
        // `unwrap` is fine here: tests may unwrap, library code may not.
        tempfile::tempdir().unwrap()
    }

    /// I2: before this, `Settings::save` had no caller and `load_or_default`
    /// never created the file, so the shipped presets — including the two
    /// `verified = false` game targets the spec requires users to be able to
    /// edit — existed only inside the compiled binary.
    #[test]
    fn a_missing_settings_file_is_created_from_the_defaults() {
        let dir = scratch_dir();
        let path = dir.path().join("settings.toml");

        let (settings, created) = Settings::load_or_create(&path).unwrap();

        assert!(created, "the call must report that it wrote the file");
        assert!(path.exists(), "the settings file must be on disk afterwards");
        assert_eq!(
            settings,
            Settings::default(),
            "what came back must equal the defaults that were written"
        );
        let text = fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("verified = false"),
            "the unverified presets must reach the file the user edits:\n{text}"
        );
    }

    #[test]
    fn an_existing_settings_file_is_loaded_and_left_alone() {
        let dir = scratch_dir();
        let path = dir.path().join("settings.toml");
        let hand_written = "interval_seconds = 900\n\n[metrics]\ndownload = false\n";
        fs::write(&path, hand_written).unwrap();

        let (settings, created) = Settings::load_or_create(&path).unwrap();

        assert!(!created);
        assert_eq!(settings.interval, Duration::from_secs(900));
        assert!(!settings.metrics.download);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            hand_written,
            "loading must not rewrite the user's file"
        );
    }

    /// A hand edit that does not parse must stay on disk exactly as the user
    /// left it — replacing it with defaults would destroy the only copy of
    /// whatever they were trying to write.
    #[test]
    fn a_corrupt_settings_file_is_an_error_and_is_never_overwritten() {
        let dir = scratch_dir();
        let path = dir.path().join("settings.toml");
        let broken = "interval_seconds = not-a-number\n";
        fs::write(&path, broken).unwrap();

        let result = Settings::load_or_create(&path);

        assert!(result.is_err(), "a corrupt file must not load");
        assert_eq!(fs::read_to_string(&path).unwrap(), broken);
    }

    /// I3: the atomicity property, stated as behaviour. The scratch path is
    /// occupied by a directory, so the scratch write cannot succeed — which
    /// stands in for any mid-write failure (full disk, power cut, crash).
    /// An atomic save reports the error and leaves the old settings intact;
    /// `fs::write` straight onto the destination would already have
    /// truncated them before it could fail.
    #[test]
    fn a_save_that_cannot_complete_leaves_the_previous_settings_intact() {
        let dir = scratch_dir();
        let path = dir.path().join("settings.toml");
        let previous = "interval_seconds = 1200\n";
        fs::write(&path, previous).unwrap();
        fs::create_dir(scratch_path_for(&path)).unwrap();

        let result = Settings::default().save(&path);

        assert!(result.is_err(), "a save that cannot write must report it");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            previous,
            "the destination was modified even though the save failed"
        );
    }

    #[test]
    fn a_successful_save_replaces_the_file_and_leaves_no_scratch_behind() {
        let dir = scratch_dir();
        let path = dir.path().join("settings.toml");
        fs::write(&path, "interval_seconds = 1200\n").unwrap();

        Settings::default().save(&path).unwrap();

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("interval_seconds = 600"));
        assert!(!text.contains("1200"), "old content survived the rename");
        assert!(
            !scratch_path_for(&path).exists(),
            "the scratch file must be gone after the rename"
        );
    }

    #[test]
    fn the_scratch_file_sits_next_to_its_destination() {
        let path = Path::new("A:/somewhere/Alidade/settings.toml");
        assert_eq!(scratch_path_for(path).parent(), path.parent());
        assert_ne!(scratch_path_for(path), path);
    }

    /// `daily_budget_bytes = none` is not valid TOML, so a user who opened
    /// the file with a real TOML tool could not save it back.
    #[test]
    fn no_budget_is_written_as_valid_toml_not_as_a_none_literal() {
        let text = Settings::default().to_toml();
        assert!(
            !text.contains("= none"),
            "invalid TOML literal still emitted:\n{text}"
        );
        assert!(
            !text.lines().any(|l| l.trim_start().starts_with("daily_budget_bytes")),
            "no budget must mean no key:\n{text}"
        );
        assert_eq!(Settings::parse(&text).unwrap(), Settings::default());
    }

    #[test]
    fn a_budget_round_trips_and_the_legacy_none_literal_still_reads() {
        let with_budget = Settings {
            daily_budget_bytes: Some(5_000_000_000),
            ..Settings::default()
        };
        let text = with_budget.to_toml();
        assert!(text.contains("daily_budget_bytes = 5000000000"));
        assert_eq!(Settings::parse(&text).unwrap(), with_budget);

        // Files written before this fix must keep loading.
        let legacy = "interval_seconds = 600\ndaily_budget_bytes = none\n";
        assert_eq!(Settings::parse(legacy).unwrap().daily_budget_bytes, None);
    }
}
