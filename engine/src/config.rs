//! User-editable settings and the shipped, explicitly unverified game presets.

use std::fs;
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
    /// Load a TOML settings file, returning defaults when it has not been created yet.
    pub fn load_or_default(path: &Path) -> Result<Self, EngineError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(path)?;
        Self::parse(&content)
    }

    /// Default `%APPDATA%\\Alidade\\settings.toml` location (with a portable fallback).
    pub fn default_path() -> PathBuf {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("Alidade").join("settings.toml")
    }

    /// Save an editable TOML file. The two game presets retain their provenance comments.
    pub fn save(&self, path: &Path) -> Result<(), EngineError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, self.to_toml())?;
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
            "interval_seconds = {}\ndaily_budget_bytes = {}\nraw_ping_retention_days = {}\n\n[endpoints]\ndownload_url = \"{}\"\nupload_url = \"{}\"\n\n[metrics]\ndownload = {}\nupload = {}\nping = {}\n",
            self.interval.as_secs(), option_u64(self.daily_budget_bytes), self.raw_ping_retention_days,
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

fn option_u64(value: Option<u64>) -> String {
    value
        .map(|n| n.to_string())
        .unwrap_or_else(|| "none".to_string())
}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
