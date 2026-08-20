//! Command-line acceptance harness for Alidade's engine and store.

use std::error::Error;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::{Duration, SystemTime};

use alidade_engine::{
    probe_once, run_round, CloudflareProvider, LoadWindow, MetricSelection, PingSample,
    PingStats, Probe, RoundKind, Scheduler, Settings, TargetSpec,
};
use alidade_store::Store;
use chrono::{NaiveDate, TimeZone, Utc};
use clap::{Args, Parser, Subcommand};

const IDLE_PING: Duration = Duration::from_secs(3);
const PHASE_BUDGET: Duration = Duration::from_secs(10);
const PING_INTERVAL: Duration = Duration::from_secs(1);

/// How often `continuous` re-runs the retention/downsample job (F4). Once a
/// day is plenty: `raw_ping_retention_days` is a matter of days, and the
/// query itself is cheap (an indexed range scan) even run more often than
/// this, so there is no accuracy reason to run it more.
const RETENTION_INTERVAL: Duration = Duration::from_secs(86_400);

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
    /// Ping configured targets once a second and print each answer. Every
    /// sample taken — a miss included, as loss — is stored in
    /// `ping_samples` (spec D5/D7, priority B: dense latency history).
    ///
    /// `ping` stays as an alias: it was the name in the first release.
    #[command(name = "ping-monitor", alias = "ping")]
    PingMonitor {
        /// Configured target name, or any part of it. Repeatable
        /// (`--target a --target b`); omit to monitor every enabled target
        /// (spec D5 says "targets", plural).
        #[arg(long)]
        target: Vec<String>,
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
        Command::PingMonitor { target, seconds } => {
            let store = open_store()?;
            ping_monitor(&settings, &target, seconds, PING_INTERVAL, &store, boxed_probe_once).await;
        }
        Command::Export { from, to, out } => export(&from, &to, &out)?,
        Command::ProbeTargets => probe_targets(&settings).await,
    }
    Ok(())
}

/// F3: a round's own idle/under-load ping samples are NOT also written to
/// `ping_samples` here (only `insert_round` is called — never
/// `insert_ping_samples`). This was a real choice, not an oversight:
///
/// - Volume alone does not force the answer either way. At the shipped
///   cadence (`throughput_every` 1h, `ping_every` 5m, `IDLE_PING` 3s and
///   `PHASE_BUDGET` 10s per phase, `PING_INTERVAL` 1s — see the constants
///   above and `engine/src/config.rs::Settings::default`) a full round's
///   ping loop samples idle (~4, t=0..3s) + down (~11, t=0..10s) + up
///   (~11) ~= 26 samples; 24 full rounds/day ~= 624. A ping-only round
///   samples idle alone, ~4; of the 288 five-minute slots/day, 24 coincide
///   with a full round and are replaced by it (`Scheduler::kind_due`), so
///   264 ping-only rounds/day ~= 1,056. Sum ~= 1,680 rows/day either way —
///   downsample already bounds the long-run cost (`raw_ping_retention_days`,
///   default 30), so even kept forever raw this is ~50k rows, trivial for
///   SQLite.
/// - The real reason is what the numbers would MEAN once mixed. A round's
///   under-load samples are deliberately taken while the link is loaded
///   (that is the whole point of `ping_down`/`ping_up`) and read
///   meaningfully higher under bufferbloat — see the D5a evidence table in
///   the spec. `ping-monitor`'s dense samples (F1/F2, this file) are
///   deliberately taken on an otherwise-idle link. Folding both into one
///   `target` history would let an hourly throughput round's bufferbloat
///   spike land in the same `ping_minute` bucket as ninety-nine idle-link
///   samples and quietly bias it — exactly the kind of wrong-signed number
///   D5a already had to fix once (see `docs/superpowers/specs/
///   2026-08-18-continuous-speed-test-design.md`).
/// - `rounds` already stores what a round measured, at the granularity a
///   round needs: `ping_idle_ms`/`ping_down_ms`/`ping_up_ms` plus per-phase
///   jitter/loss (schema v2). The raw per-sample series behind those
///   aggregates has no consumer once the aggregate is computed; if one ever
///   shows up (e.g. a per-round detail view), it should be a query scoped to
///   that round's id, not a merge into the target-keyed dense-history table.
///
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
    // One provider for the whole run: a 429's backoff lives on the
    // instance, and a fresh provider each round would forget it and hit
    // the same limit again.
    let provider = CloudflareProvider::new(settings.endpoints.clone());
    println!(
        "continuous mode; full round every {}, ping-only round every {}; press Ctrl+C to stop",
        format_duration(settings.throughput_every),
        format_duration(settings.ping_every)
    );
    // F4: the retention/downsample job (`store/src/retention.rs`) has a
    // caller now. `continuous` is the only long-running production path
    // today — a one-shot `single`/`ping-monitor` exits before a day could
    // ever pass — so this is where it belongs; D8's tray-resident background
    // mode inherits it once it exists, rather than needing its own copy.
    // Once at start-up (cheap and idempotent even when nothing is old
    // enough yet) and then once per `RETENTION_INTERVAL` while the loop runs.
    if let Err(err) = store.downsample_pings(settings.raw_ping_retention_days) {
        eprintln!("retention: {err}");
    }
    let mut last_retention = SystemTime::now();
    loop {
        let started = SystemTime::now();
        let plan = scheduler.plan_next_round(started);
        scheduler.record_round_start(started, plan.kind);
        let metrics = intersect_metrics(plan.metrics, metrics_for(&settings, &metric_args));
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
        // F3: only the round's own aggregate columns are stored here — see
        // the comment on `run_and_store` for why its raw idle/under-load
        // ping samples do not also go to `insert_ping_samples`.
        store.insert_round(&result)?;
        print_round(&result);
        if started.duration_since(last_retention).unwrap_or(Duration::ZERO) >= RETENTION_INTERVAL {
            if let Err(err) = store.downsample_pings(settings.raw_ping_retention_days) {
                eprintln!("retention: {err}");
            }
            last_retention = started;
        }
        let now = SystemTime::now();
        let wait = scheduler
            .next_due(now)
            .duration_since(now)
            .unwrap_or(Duration::ZERO);
        tokio::select! { _ = tokio::signal::ctrl_c() => break, _ = tokio::time::sleep(wait) => {} }
    }
    Ok(())
}

/// A prober callable, boxed the same way `boxed_probe_once` boxes the real
/// one. A plain fn pointer (not a capturing closure) so `run_ping_monitor`
/// can hand it to a spawned task without extra bounds, and so a test can
/// swap in a fake with no network in it — the same seam
/// `icmp_rtt_bounded`'s injected resolver uses in `probe.rs`.
type ProbeFn = fn(Probe, Duration) -> Pin<Box<dyn Future<Output = PingSample> + Send>>;

fn boxed_probe_once(probe: Probe, timeout: Duration) -> Pin<Box<dyn Future<Output = PingSample> + Send>> {
    Box::pin(async move { probe_once(&probe, timeout).await })
}

/// Which targets `ping-monitor` should watch: the named subset, resolved
/// once before the first tick so a misspelled name is a startup error
/// rather than something that only shows up after minutes of silent output,
/// or — with none named — every enabled target (spec D5, "targets" is
/// plural; F2).
fn resolve_ping_targets<'a>(
    settings: &'a Settings,
    wanted: &[String],
) -> Result<Vec<&'a TargetSpec>, String> {
    if wanted.is_empty() {
        return Ok(settings.targets.iter().filter(|t| t.enabled).collect());
    }
    wanted
        .iter()
        .map(|name| {
            settings
                .targets
                .iter()
                .find(|t| {
                    t.enabled
                        && (t.name.eq_ignore_ascii_case(name)
                            || t.name.to_ascii_lowercase().contains(&name.to_ascii_lowercase()))
                })
                .ok_or_else(|| format!("unknown configured target: {name}"))
        })
        .collect()
}

/// F1/F2: resolve the requested targets, then run the shared monitoring
/// loop. This is the function `main` dispatches to; `prober` is always
/// `boxed_probe_once` in production and a fake in tests, so the same call
/// path (including the store write) is what both exercise.
async fn ping_monitor(
    settings: &Settings,
    wanted: &[String],
    seconds: u64,
    interval: Duration,
    store: &Store,
    prober: ProbeFn,
) {
    let targets = match resolve_ping_targets(settings, wanted) {
        Ok(targets) => targets,
        Err(message) => {
            eprintln!("{message}");
            return;
        }
    };
    if targets.is_empty() {
        eprintln!("no enabled targets to monitor");
        return;
    }
    run_ping_monitor(&targets, seconds.max(1), interval, store, prober).await;
}

/// F1/F2 core. Every tick probes every target AT ONCE (`JoinSet`, not a
/// sequential loop) so one dead target's full-timeout loss costs the tick
/// nothing beyond what it would have cost alone — proven by the LoL EUNE
/// preset (`104.160.142.3:443`), confirmed dead, in
/// `every_target_is_probed_concurrently_so_a_dead_one_does_not_delay_the_rest`.
///
/// Every sample this loop takes reaches `store.insert_ping_samples` before
/// the next tick starts, including a miss: `PingSample.rtt: None` is loss,
/// stored as SQL NULL (see `ping_avg` in `alidade-store`), never as a gap in
/// the table and never as `0.0`.
async fn run_ping_monitor(
    targets: &[&TargetSpec],
    ticks: u64,
    interval: Duration,
    store: &Store,
    prober: ProbeFn,
) {
    for _ in 0..ticks {
        let mut set = tokio::task::JoinSet::new();
        for target in targets {
            let name = target.name.clone();
            let probe = target.probe.clone();
            set.spawn(async move { (name, prober(probe, interval).await) });
        }
        while let Some(joined) = set.join_next().await {
            let Ok((name, sample)) = joined else {
                continue; // a probe task panicked; nothing to store or print for it
            };
            match sample.rtt {
                Some(rtt) => println!("{name}: {:.1} ms", rtt.as_secs_f64() * 1000.0),
                None => println!("{name}: no answer"),
            }
            if let Err(err) = store.insert_ping_samples(&name, std::slice::from_ref(&sample)) {
                eprintln!("failed to store ping sample for {name}: {err}");
            }
        }
        tokio::time::sleep(interval).await;
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
                    assert_eq!(target, vec!["cloudflare".to_string()]);
                    assert_eq!(seconds, 30);
                }
                other => panic!("expected the ping monitor, got {other:?}"),
            }
        }
    }

    /// F2: the flag is repeatable and empty by default — "several targets,
    /// or all enabled ones" (spec D5).
    #[test]
    fn target_is_repeatable_and_defaults_to_empty_meaning_every_enabled_target() {
        let none = Cli::try_parse_from(["alidade", "ping-monitor"]).unwrap();
        let several = Cli::try_parse_from([
            "alidade",
            "ping-monitor",
            "--target",
            "cloudflare",
            "--target",
            "lol",
        ])
        .unwrap();
        match none.command {
            Command::PingMonitor { target, .. } => assert!(target.is_empty()),
            other => panic!("expected the ping monitor, got {other:?}"),
        }
        match several.command {
            Command::PingMonitor { target, .. } => {
                assert_eq!(target, vec!["cloudflare".to_string(), "lol".to_string()]);
            }
            other => panic!("expected the ping monitor, got {other:?}"),
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

    // --- F1/F2: ping-monitor persistence -----------------------------

    fn test_target(name: &str, host: &str) -> TargetSpec {
        TargetSpec {
            name: name.to_string(),
            probe: Probe::Icmp { host: host.to_string() },
            verified: true,
            enabled: true,
        }
    }

    /// A no-network stand-in for `boxed_probe_once`: any target whose host
    /// is `DEAD_HOST` is a miss, everything else answers instantly. Real
    /// network in a unit test is explicitly out per the task brief.
    const DEAD_HOST: &str = "dead.invalid";

    fn fake_prober(probe: Probe, _timeout: Duration) -> Pin<Box<dyn Future<Output = PingSample> + Send>> {
        Box::pin(async move {
            let host = match &probe {
                Probe::Icmp { host } | Probe::TcpConnect { host, .. } => host.clone(),
            };
            let rtt = if host == DEAD_HOST {
                None
            } else {
                Some(Duration::from_millis(20))
            };
            PingSample { at: SystemTime::now(), rtt }
        })
    }

    /// The finding this task exists to fix, reproduced as a test that would
    /// have caught it: `ping_monitor` is the exact function `main` calls
    /// for the `ping-monitor` command (see the dispatch in `main`), with
    /// only the network-touching `prober` swapped for `fake_prober` — the
    /// same seam `icmp_rtt_bounded` uses for its injected resolver in
    /// `probe.rs`. A test that inserted its own fixtures into
    /// `ping_samples`, the way the store crate's retention tests do, would
    /// have stayed green for 71 rounds while nothing called the write path;
    /// this one drives `ping_monitor` and only then looks at the table.
    ///
    /// Mutation-checked: with the `store.insert_ping_samples(...)` call
    /// removed from `run_ping_monitor`, this test fails with
    /// `assertion `left == right` failed: one sample per tick reached ping_samples`
    /// `  left: 0`
    /// `  right: 3`
    /// — see `task-7d-report.md` for the captured run. Restoring the call
    /// makes it pass again.
    #[tokio::test]
    async fn ping_monitor_persists_every_sample_it_takes() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("a.db")).unwrap();
        let settings = Settings {
            targets: vec![test_target("Alpha", "1.2.3.4")],
            ..Settings::default()
        };

        ping_monitor(&settings, &[], 3, Duration::from_millis(5), &store, fake_prober).await;

        assert_eq!(
            store.ping_sample_count().unwrap(),
            3,
            "one sample per tick reached ping_samples"
        );
    }

    /// F2: with several targets monitored (the default — no `--target`
    /// named, so every enabled target), each row lands under its own
    /// `target`, not under whichever target happened to run first or last.
    #[tokio::test]
    async fn samples_are_attributed_to_the_right_target_when_several_are_monitored() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("a.db")).unwrap();
        let settings = Settings {
            targets: vec![test_target("Alpha", "1.2.3.4"), test_target("Beta", "5.6.7.8")],
            ..Settings::default()
        };

        ping_monitor(&settings, &[], 2, Duration::from_millis(5), &store, fake_prober).await;

        assert_eq!(store.ping_sample_rtts_for("Alpha").unwrap().len(), 2);
        assert_eq!(store.ping_sample_rtts_for("Beta").unwrap().len(), 2);
        assert_eq!(
            store.ping_sample_count().unwrap(),
            4,
            "no sample landed under the wrong target or a phantom third one"
        );
    }

    /// F1: a target that never answers is loss, stored as SQL NULL — never
    /// omitted (a gap in the table would read as "nobody looked") and never
    /// `0.0` (which would read as the best possible link).
    #[tokio::test]
    async fn a_non_answering_target_is_stored_as_loss_not_as_zero() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("a.db")).unwrap();
        let settings = Settings {
            targets: vec![test_target("Dead", DEAD_HOST)],
            ..Settings::default()
        };

        ping_monitor(&settings, &[], 2, Duration::from_millis(5), &store, fake_prober).await;

        let rtts = store.ping_sample_rtts_for("Dead").unwrap();
        assert_eq!(rtts.len(), 2, "the miss must still be a row, not an omission");
        assert!(
            rtts.iter().all(Option::is_none),
            "a miss must be a recorded loss (NULL), not skipped: {rtts:?}"
        );
        assert!(
            !rtts.contains(&Some(0.0)),
            "a miss must never be stored as a perfect 0.0 ms ping: {rtts:?}"
        );
    }

    /// A prober where every target sleeps out its full timeout before
    /// reporting loss — the deterministic stand-in for the confirmed-dead
    /// LoL EUNE preset (`104.160.142.3:443`) the brief names.
    fn slow_prober(_probe: Probe, timeout: Duration) -> Pin<Box<dyn Future<Output = PingSample> + Send>> {
        Box::pin(async move {
            tokio::time::sleep(timeout).await;
            PingSample { at: SystemTime::now(), rtt: None }
        })
    }

    /// F2: targets are probed AT ONCE, not one after another. Four targets
    /// that each take a full `interval` to time out would cost `>= 4 *
    /// interval` probed sequentially before the tick's own trailing sleep
    /// even starts; probed concurrently (`run_ping_monitor`'s `JoinSet`)
    /// the tick costs about one `interval` regardless of target count.
    #[tokio::test]
    async fn every_target_is_probed_concurrently_so_a_dead_one_does_not_delay_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("a.db")).unwrap();
        let interval = Duration::from_millis(60);
        let settings = Settings {
            targets: (0..4)
                .map(|i| test_target(&format!("Dead{i}"), DEAD_HOST))
                .collect(),
            ..Settings::default()
        };

        let start = std::time::Instant::now();
        ping_monitor(&settings, &[], 1, interval, &store, slow_prober).await;
        let elapsed = start.elapsed();

        assert!(
            elapsed < interval * 3,
            "elapsed {elapsed:?} for 4 targets each bounded by {interval:?} suggests \
             sequential probing, not concurrent (sequential would cost >= 4 * interval \
             for the probes alone, before the tick's own trailing sleep)"
        );
        assert_eq!(store.ping_sample_count().unwrap(), 4, "still one row per target");
    }
}
