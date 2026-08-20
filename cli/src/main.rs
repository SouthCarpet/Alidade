//! Command-line acceptance harness for Alidade's engine and store.

use std::error::Error;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use alidade_engine::{
    probe_once, run_round, CloudflareProvider, LoadWindow, MetricSelection, PingStats, Probe,
    RoundKind, Scheduler, Settings,
};
use alidade_store::Store;
use chrono::{NaiveDate, TimeZone, Utc};
use clap::{Args, Parser, Subcommand};

const IDLE_PING: Duration = Duration::from_secs(3);
const PHASE_BUDGET: Duration = Duration::from_secs(10);
const PING_INTERVAL: Duration = Duration::from_secs(1);

/// Last second of a UTC day, added to a `--to` date so the whole day counts.
const LAST_SECOND_OF_DAY: u64 = 86_400 - 1;

/// `--interval` was one cadence; continuous mode has two now (D5a). Mapping
/// it onto either would leave the other at its default and quietly change
/// what the user asked for, so the flag is an error that names both
/// replacements instead.
const INTERVAL_REPLACED: &str = "--interval is gone: continuous mode runs two cadences now. \
     Use --throughput-every <DURATION> for full rounds (default 1h) and \
     --ping-every <DURATION> for ping-only rounds (default 5m).";

#[derive(Parser, Debug)]
#[command(
    name = "alidade",
    version,
    about = "Measure the internet link and keep the record.",
    long_about = "Alidade runs time-aligned measurement rounds (download, upload, ping and \
                  ping-under-load) against a speed endpoint and per-target probes, and stores \
                  every round in SQLite."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run one measurement round now and store it.
    Single {
        #[command(flatten)]
        metrics: MetricArgs,
    },
    /// Run both cadences until stopped with Ctrl+C: full rounds every
    /// `--throughput-every`, ping-only rounds every `--ping-every`.
    Continuous {
        #[command(flatten)]
        metrics: MetricArgs,
        /// Gap between full round starts, e.g. 30m, 1h, 6h. Minimum 1 minute.
        #[arg(long, value_name = "DURATION", value_parser = parse_duration)]
        throughput_every: Option<Duration>,
        /// Gap between ping-only round starts, e.g. 60s, 5m. Minimum 1 minute.
        #[arg(long, value_name = "DURATION", value_parser = parse_duration)]
        ping_every: Option<Duration>,
        /// Removed — see `INTERVAL_REPLACED`. Kept as a hidden argument so
        /// the old flag gets an answer that names its replacements instead of
        /// clap's generic "unexpected argument".
        #[arg(
            long,
            hide = true,
            num_args = 0..=1,
            default_missing_value = "",
            value_name = "DURATION"
        )]
        interval: Option<String>,
    },
    /// Ping one configured target once a second and print each answer.
    ///
    /// `ping` stays as an alias: it was the name in the first release.
    #[command(name = "ping-monitor", alias = "ping")]
    PingMonitor {
        /// Configured target name, or any part of it.
        #[arg(long, default_value = "google")]
        target: String,
        /// How many samples to take.
        #[arg(long, default_value_t = 30)]
        seconds: u64,
    },
    /// Write the stored rounds of a date range to a CSV file.
    Export {
        /// First day to include, UTC (YYYY-MM-DD).
        #[arg(long, value_name = "YYYY-MM-DD")]
        from: String,
        /// Last day to include, UTC (YYYY-MM-DD). The whole day counts.
        #[arg(long, value_name = "YYYY-MM-DD")]
        to: String,
        /// File to write.
        #[arg(long, value_name = "PATH")]
        out: PathBuf,
    },
    /// Probe every enabled target once and report which ones answer.
    ProbeTargets,
}

/// Which metrics a round runs. At most one of these may be given — they are
/// three ways of saying the same thing, and combining them is always a
/// mistake rather than an intent clap could guess.
#[derive(Args, Debug, Default, Clone, Copy)]
#[group(multiple = false)]
struct MetricArgs {
    /// Measure download and ping, skip upload.
    #[arg(long)]
    no_upload: bool,
    /// Measure upload and ping, skip download.
    #[arg(long)]
    no_download: bool,
    /// Measure ping only, no throughput.
    #[arg(long)]
    ping_only: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let settings_path = Settings::default_path();
    let (settings, created) = Settings::load_or_create(&settings_path)?;
    if created {
        println!("wrote default settings to {}", settings_path.display());
    }
    match cli.command {
        Command::Single { metrics } => {
            run_and_store(&settings, metrics_for(&settings, &metrics)).await?
        }
        Command::Continuous {
            metrics,
            throughput_every,
            ping_every,
            interval,
        } => {
            if interval.is_some() {
                return Err(INTERVAL_REPLACED.into());
            }
            continuous(settings, metrics, throughput_every, ping_every).await?
        }
        Command::PingMonitor { target, seconds } => ping_monitor(&settings, &target, seconds).await,
        Command::Export { from, to, out } => export(&from, &to, &out)?,
        Command::ProbeTargets => probe_targets(&settings).await,
    }
    Ok(())
}

/// A one-shot round is never byte-capped: the daily budget governs the
/// unattended cadence, and truncating a test the user asked for by hand would
/// hand them the short window this release exists to remove.
async fn run_and_store(
    settings: &Settings,
    metrics: MetricSelection,
) -> Result<(), Box<dyn Error>> {
    let provider = CloudflareProvider::new(settings.endpoints.clone());
    let result = run_round(&provider, &round_config(settings, metrics, None)).await;
    let store = open_store()?;
    store.insert_round(&result)?;
    print_round(&result);
    Ok(())
}

async fn continuous(
    mut settings: Settings,
    metric_args: MetricArgs,
    throughput_every: Option<Duration>,
    ping_every: Option<Duration>,
) -> Result<(), Box<dyn Error>> {
    if let Some(every) = throughput_every {
        settings.throughput_every = every;
    }
    if let Some(every) = ping_every {
        settings.ping_every = every;
    }
    let scheduler = Scheduler::new(settings.clone());
    // One connection for the whole run: re-opening the database every round
    // re-ran the migration check and re-took the file lock for no gain.
    let store = open_store()?;
    println!(
        "continuous mode; full round every {}, ping-only round every {}; press Ctrl+C to stop",
        format_duration(settings.throughput_every),
        format_duration(settings.ping_every)
    );
    loop {
        let started = SystemTime::now();
        let plan = scheduler.plan_next_round(started);
        scheduler.record_round_start(started, plan.kind);
        let metrics = intersect_metrics(plan.metrics, metrics_for(&settings, &metric_args));
        let provider = CloudflareProvider::new(settings.endpoints.clone());
        let mut result =
            run_round(&provider, &round_config(&settings, metrics, plan.byte_ceiling)).await;
        if result.skipped_reason.is_none() {
            result.skipped_reason = plan.skip_reason;
        }
        let bytes = result
            .down
            .map_or(0, |throughput| throughput.bytes)
            .saturating_add(result.up.map_or(0, |throughput| throughput.bytes));
        scheduler.record_bytes(started, bytes);
        store.insert_round(&result)?;
        print_round(&result);
        let now = SystemTime::now();
        let wait = scheduler
            .next_due(now)
            .duration_since(now)
            .unwrap_or(Duration::ZERO);
        tokio::select! { _ = tokio::signal::ctrl_c() => break, _ = tokio::time::sleep(wait) => {} }
    }
    Ok(())
}

async fn ping_monitor(settings: &Settings, wanted: &str, seconds: u64) {
    let target = settings.targets.iter().find(|target| {
        target.name.eq_ignore_ascii_case(wanted)
            || target
                .name
                .to_ascii_lowercase()
                .contains(&wanted.to_ascii_lowercase())
    });
    let Some(target) = target else {
        eprintln!("unknown configured target: {wanted}");
        return;
    };
    for _ in 0..seconds.max(1) {
        let sample = probe_once(&target.probe, PING_INTERVAL).await;
        match sample.rtt {
            Some(rtt) => println!("{}: {:.1} ms", target.name, rtt.as_secs_f64() * 1000.0),
            None => println!("{}: no answer", target.name),
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn probe_targets(settings: &Settings) {
    println!("name\tkind\thost\tanswered?\trtt");
    for target in settings.targets.iter().filter(|target| target.enabled) {
        let (kind, host) = probe_description(&target.probe);
        let sample = probe_once(&target.probe, Duration::from_secs(3)).await;
        let (answered, rtt) = match sample.rtt {
            Some(rtt) => ("yes", format!("{:.1} ms", rtt.as_secs_f64() * 1000.0)),
            None => ("no", "-".to_string()),
        };
        println!("{}\t{}\t{}\t{}\t{}", target.name, kind, host, answered, rtt);
    }
}

fn export(from: &str, to: &str, out: &std::path::Path) -> Result<(), Box<dyn Error>> {
    let store = open_store()?;
    let rows = store.export_rounds_csv(parse_day_start(from)?, parse_day_end(to)?, out)?;
    println!("exported {rows} rounds to {}", out.display());
    Ok(())
}

fn round_config(
    settings: &Settings,
    metrics: MetricSelection,
    byte_ceiling: Option<u64>,
) -> alidade_engine::RoundConfig {
    alidade_engine::RoundConfig {
        metrics,
        idle_ping: IDLE_PING,
        phase_budget: PHASE_BUDGET,
        byte_ceiling,
        ping_interval: PING_INTERVAL,
        targets: settings
            .targets
            .iter()
            .filter(|target| target.enabled)
            .map(|target| target.probe.clone())
            .collect(),
    }
}

fn metrics_for(settings: &Settings, args: &MetricArgs) -> MetricSelection {
    if args.ping_only {
        MetricSelection {
            download: false,
            upload: false,
            ping: settings.metrics.ping,
        }
    } else {
        MetricSelection {
            download: settings.metrics.download && !args.no_download,
            upload: settings.metrics.upload && !args.no_upload,
            ping: settings.metrics.ping,
        }
    }
}

fn intersect_metrics(left: MetricSelection, right: MetricSelection) -> MetricSelection {
    MetricSelection {
        download: left.download && right.download,
        upload: left.upload && right.upload,
        ping: left.ping && right.ping,
    }
}

/// A ping-only round has no throughput to report and must not be printed as
/// a full round with two skipped phases; a capped round must not be printed
/// as an ordinary one, because its speeds came from a window the data budget
/// cut short.
fn print_round(result: &alidade_engine::RoundResult) {
    if result.kind == RoundKind::PingOnly {
        println!("ping-only round; ping: {}", format_ping(result.ping_idle));
    } else {
        let down = format_speed(result.down);
        let up = format_speed(result.up);
        println!(
            "download: {down}; upload: {up}; idle ping: {}",
            format_ping(result.ping_idle)
        );
        println!(
            "ping under load: download {}; upload {}",
            format_under_load(result.ping_down, result.down_load),
            format_under_load(result.ping_up, result.up_load)
        );
    }
    if result.capped {
        println!("capped: the daily data budget, not the clock, ended a throughput phase");
    }
    if let Some(reason) = &result.skipped_reason {
        println!("skip reason: {reason}");
    }
}

fn format_speed(throughput: Option<alidade_engine::Throughput>) -> String {
    throughput.map_or_else(
        || "skipped".to_string(),
        |t| format!("{:.2} Mbit/s", t.bits_per_sec / 1_000_000.0),
    )
}

/// A phase where nothing answered has no RTT to print. Saying `0.0 ms` there
/// would read as the best possible link instead of the worst.
fn format_ping(stats: Option<PingStats>) -> String {
    match stats {
        None => "skipped".to_string(),
        Some(stats) => match stats.avg_ms {
            Some(avg_ms) => format!("{avg_ms:.1} ms"),
            None => format!("no answer ({:.0}% loss)", stats.loss_pct),
        },
    }
}

/// Under-load ping, with the two reasons for an absent number kept apart: the
/// phase never ran, or it ran with too little load behind it to mean
/// anything (see `MIN_UNDER_LOAD_SAMPLES`). Printing both as `skipped` would
/// hide exactly the case this release is about.
fn format_under_load(stats: Option<PingStats>, window: Option<LoadWindow>) -> String {
    match (stats, window) {
        (Some(stats), _) => format_ping(Some(stats)),
        (None, Some(window)) => format!(
            "not enough load to measure ({} sample(s) in {:.1}s of load)",
            window.ping_samples,
            window.duration.as_secs_f64()
        ),
        (None, None) => "skipped".to_string(),
    }
}

/// Cadence as the user writes it (`1h`, `5m`, `90s`) — the same spelling
/// `parse_duration` accepts, so the line can be pasted back as a flag.
fn format_duration(value: Duration) -> String {
    let seconds = value.as_secs();
    if seconds.is_multiple_of(3600) {
        format!("{}h", seconds / 3600)
    } else if seconds.is_multiple_of(60) {
        format!("{}m", seconds / 60)
    } else {
        format!("{seconds}s")
    }
}

fn probe_description(probe: &Probe) -> (&'static str, String) {
    match probe {
        Probe::Icmp { host } => ("icmp", host.clone()),
        Probe::TcpConnect { host, port } => ("tcp", format!("{host}:{port}")),
    }
}

fn default_db_path() -> PathBuf {
    let base = std::env::var_os("ALIDADE_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Alidade").join("alidade.db")
}

fn open_store() -> Result<Store, Box<dyn Error>> {
    let path = default_db_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(Store::open(&path)?)
}

fn parse_duration(value: &str) -> Result<Duration, String> {
    let split = value
        .find(|character: char| !character.is_ascii_digit())
        .ok_or_else(|| "cadence needs a unit (s, m, or h)".to_string())?;
    let (number, unit) = value.split_at(split);
    let quantity: u64 = number
        .parse()
        .map_err(|_| "cadence must start with a whole number".to_string())?;
    let seconds = match unit {
        "s" => quantity,
        "m" => quantity.saturating_mul(60),
        "h" => quantity.saturating_mul(3600),
        _ => return Err("cadence unit must be s, m, or h".to_string()),
    };
    if Duration::from_secs(seconds) < alidade_engine::MIN_CADENCE {
        return Err("cadence must be at least 1 minute".to_string());
    }
    Ok(Duration::from_secs(seconds))
}

/// Midnight UTC at the start of `value` — the first instant of that day.
fn parse_day_start(value: &str) -> Result<SystemTime, Box<dyn Error>> {
    Ok(SystemTime::UNIX_EPOCH + Duration::from_secs(day_start_secs(value)?))
}

/// The LAST instant of `value`, not the first. `rounds_between` is inclusive
/// on both ends, so `--to 2026-08-20` resolved to that day's midnight
/// excluded everything measured on 2026-08-20 after 00:00:00 — the day the
/// user named was the one day missing from the export.
fn parse_day_end(value: &str) -> Result<SystemTime, Box<dyn Error>> {
    Ok(SystemTime::UNIX_EPOCH
        + Duration::from_secs(day_start_secs(value)?.saturating_add(LAST_SECOND_OF_DAY)))
}

fn day_start_secs(value: &str) -> Result<u64, Box<dyn Error>> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")?;
    let midnight = date
        .and_hms_opt(0, 0, 0)
        .ok_or("date cannot represent midnight")?;
    let seconds = Utc.from_utc_datetime(&midnight).timestamp();
    Ok(u64::try_from(seconds).map_err(|_| "date is before the Unix epoch")?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn secs(t: SystemTime) -> u64 {
        t.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()
    }

    #[test]
    fn the_command_tree_is_well_formed() {
        Cli::command().debug_assert();
    }

    /// The hand-rolled parser this replaced answered `--help` with
    /// `Error: "unknown command or arguments: --help"`.
    #[test]
    fn help_and_version_are_answered_not_rejected_as_unknown() {
        let help = Cli::try_parse_from(["alidade", "--help"]).unwrap_err();
        assert_eq!(help.kind(), clap::error::ErrorKind::DisplayHelp);
        let version = Cli::try_parse_from(["alidade", "--version"]).unwrap_err();
        assert_eq!(version.kind(), clap::error::ErrorKind::DisplayVersion);
        let sub = Cli::try_parse_from(["alidade", "single", "--help"]).unwrap_err();
        assert_eq!(sub.kind(), clap::error::ErrorKind::DisplayHelp);
    }

    /// The hand-rolled parser ignored unknown flags on `single` and
    /// `continuous` and started a live measurement round anyway.
    #[test]
    fn an_unknown_flag_is_an_error_not_a_silent_measurement() {
        let err = Cli::try_parse_from(["alidade", "single", "--no-uploads"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::UnknownArgument);
        assert!(Cli::try_parse_from(["alidade", "continuous", "--nope"]).is_err());
        assert!(Cli::try_parse_from(["alidade", "probe-targets", "--nope"]).is_err());
    }

    #[test]
    fn the_metric_flags_stay_mutually_exclusive() {
        assert!(Cli::try_parse_from(["alidade", "single", "--no-upload"]).is_ok());
        assert!(
            Cli::try_parse_from(["alidade", "single", "--no-upload", "--ping-only"]).is_err(),
            "two ways of selecting metrics at once must still be refused"
        );
    }

    /// Minor item: the plan calls this command `ping-monitor`; the first
    /// release shipped it as `ping`. The name is `ping-monitor`, and `ping`
    /// keeps working so nothing already written breaks.
    #[test]
    fn ping_monitor_answers_to_its_old_name_too() {
        let new = Cli::try_parse_from(["alidade", "ping-monitor", "--target", "cloudflare"]);
        let old = Cli::try_parse_from(["alidade", "ping", "--target", "cloudflare"]);
        for parsed in [new, old] {
            match parsed.unwrap().command {
                Command::PingMonitor { target, seconds } => {
                    assert_eq!(target, "cloudflare");
                    assert_eq!(seconds, 30);
                }
                other => panic!("expected the ping monitor, got {other:?}"),
            }
        }
    }

    #[test]
    fn every_command_name_from_the_first_release_still_parses() {
        for argv in [
            vec!["alidade", "single"],
            vec!["alidade", "continuous", "--throughput-every", "1h"],
            vec!["alidade", "export", "--from", "2026-08-01", "--to", "2026-08-20", "--out", "r.csv"],
            vec!["alidade", "probe-targets"],
        ] {
            assert!(Cli::try_parse_from(&argv).is_ok(), "{argv:?} must still parse");
        }
    }

    #[test]
    fn a_cadence_under_a_minute_is_refused_on_either_flag() {
        assert!(Cli::try_parse_from(["alidade", "continuous", "--throughput-every", "30s"]).is_err());
        assert!(Cli::try_parse_from(["alidade", "continuous", "--ping-every", "30s"]).is_err());
        assert!(Cli::try_parse_from(["alidade", "continuous", "--ping-every", "10"]).is_err());
    }

    /// F4. Both cadences are settable and independent — the point of D5a is
    /// that a dense ping cadence no longer drags the expensive throughput
    /// round along with it.
    #[test]
    fn continuous_takes_both_cadences_independently() {
        let parsed =
            Cli::try_parse_from(["alidade", "continuous", "--throughput-every", "2h", "--ping-every", "60s"])
                .unwrap();
        match parsed.command {
            Command::Continuous {
                throughput_every,
                ping_every,
                interval,
                ..
            } => {
                assert_eq!(throughput_every, Some(Duration::from_secs(7200)));
                assert_eq!(ping_every, Some(Duration::from_secs(60)));
                assert_eq!(interval, None);
            }
            other => panic!("expected continuous, got {other:?}"),
        }
    }

    /// `--interval` used to mean the one cadence. Accepting it silently would
    /// set the throughput cadence and leave ping at five minutes, or the
    /// reverse — either way the user gets a schedule they did not ask for. It
    /// parses (so the message is ours, not clap's "unexpected argument") and
    /// then refuses, naming both replacements.
    #[test]
    fn the_old_interval_flag_is_refused_and_names_its_replacements() {
        for argv in [
            vec!["alidade", "continuous", "--interval", "10m"],
            vec!["alidade", "continuous", "--interval"],
        ] {
            let parsed = Cli::try_parse_from(&argv).unwrap();
            match parsed.command {
                Command::Continuous { interval, .. } => assert!(
                    interval.is_some(),
                    "{argv:?} must reach the guard, not be silently dropped"
                ),
                other => panic!("expected continuous, got {other:?}"),
            }
        }
        assert!(INTERVAL_REPLACED.contains("--throughput-every"));
        assert!(INTERVAL_REPLACED.contains("--ping-every"));
    }

    /// F4's print rule, at the level a unit test can hold it: an under-load
    /// ping that is absent because the load was too short must not read the
    /// same as one that never ran.
    #[test]
    fn an_absent_under_load_ping_says_which_kind_of_absent_it_is() {
        let no_phase = format_under_load(None, None);
        let too_little_load = format_under_load(
            None,
            Some(LoadWindow {
                duration: Duration::from_millis(4300),
                ping_samples: 2,
            }),
        );
        assert_eq!(no_phase, "skipped");
        assert!(
            too_little_load.contains("not enough load") && too_little_load.contains("2 sample"),
            "{too_little_load}"
        );
        assert_ne!(no_phase, too_little_load);
    }

    #[test]
    fn a_cadence_prints_in_the_units_its_flag_accepts() {
        for (value, text) in [
            (Duration::from_secs(3600), "1h"),
            (Duration::from_secs(300), "5m"),
            (Duration::from_secs(90), "90s"),
        ] {
            assert_eq!(format_duration(value), text);
            assert_eq!(parse_duration(text).unwrap(), value);
        }
    }

    /// The `--to` boundary: the last round of the named day is inside the
    /// range, the first round of the next day is outside it. With `--to`
    /// resolved to midnight (the old behaviour) the first assertion fails —
    /// 23:59:30 would sit 86,370 seconds past the end of the range.
    #[test]
    fn to_covers_the_whole_named_day_not_just_its_first_instant() {
        let to = parse_day_end("2026-08-20").unwrap();
        let last_round_of_that_day = parse_day_start("2026-08-20").unwrap()
            + Duration::from_secs(23 * 3600 + 59 * 60 + 30);
        let first_round_of_the_next_day = parse_day_start("2026-08-21").unwrap();

        assert!(
            last_round_of_that_day <= to,
            "23:59:30 on the --to day must be inside the export range"
        );
        assert!(
            first_round_of_the_next_day > to,
            "--to must not spill into the following day"
        );
        assert_eq!(secs(to), secs(first_round_of_the_next_day) - 1);
    }

    #[test]
    fn from_is_the_first_instant_of_its_day() {
        assert_eq!(secs(parse_day_start("1970-01-02").unwrap()), 86_400);
    }
}
