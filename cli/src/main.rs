//! Command-line acceptance harness for Alidade's engine and store.

use std::error::Error;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use alidade_engine::{
    probe_once, run_round, CloudflareProvider, MetricSelection, Probe, Scheduler, Settings,
};
use alidade_store::Store;
use chrono::{NaiveDate, TimeZone, Utc};

const IDLE_PING: Duration = Duration::from_secs(3);
const PHASE_BUDGET: Duration = Duration::from_secs(10);
const PING_INTERVAL: Duration = Duration::from_secs(1);
const MAX_BYTES_PER_PHASE: u64 = 100 * 1024 * 1024;

#[derive(Debug)]
enum Command {
    Single(MetricArgs),
    Continuous(ContinuousArgs),
    Ping(PingArgs),
    Export(ExportArgs),
    ProbeTargets,
}

#[derive(Debug, Default)]
struct MetricArgs {
    no_upload: bool,
    no_download: bool,
    ping_only: bool,
}

#[derive(Debug)]
struct ContinuousArgs {
    metrics: MetricArgs,
    interval: Option<Duration>,
}

#[derive(Debug)]
struct PingArgs {
    target: String,
    seconds: u64,
}

#[derive(Debug)]
struct ExportArgs {
    from: String,
    to: String,
    out: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let command = parse_cli()?;
    let settings = Settings::load_or_default(&Settings::default_path())?;
    match command {
        Command::Single(args) => run_and_store(&settings, metrics_for(&settings, &args)).await?,
        Command::Continuous(args) => continuous(settings, args).await?,
        Command::Ping(args) => ping(&settings, args).await,
        Command::Export(args) => export(args)?,
        Command::ProbeTargets => probe_targets(&settings).await,
    }
    Ok(())
}

fn parse_cli() -> Result<Command, Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let command = args
        .next()
        .ok_or("usage: alidade <single|continuous|ping|export|probe-targets>")?;
    let rest: Vec<String> = args.collect();
    match command.as_str() {
        "single" => Ok(Command::Single(parse_metrics(&rest)?)),
        "continuous" => {
            let metrics = parse_metrics(&rest)?;
            let interval = option_value(&rest, "--interval")
                .map(parse_duration)
                .transpose()?;
            Ok(Command::Continuous(ContinuousArgs { metrics, interval }))
        }
        "ping" => {
            let target = option_value(&rest, "--target")
                .unwrap_or("google")
                .to_string();
            let seconds = option_value(&rest, "--seconds")
                .map(str::parse)
                .transpose()?
                .unwrap_or(30);
            Ok(Command::Ping(PingArgs { target, seconds }))
        }
        "export" => Ok(Command::Export(ExportArgs {
            from: required_option(&rest, "--from")?.to_string(),
            to: required_option(&rest, "--to")?.to_string(),
            out: PathBuf::from(required_option(&rest, "--out")?),
        })),
        "probe-targets" if rest.is_empty() => Ok(Command::ProbeTargets),
        _ => Err(format!("unknown command or arguments: {command}").into()),
    }
}

fn parse_metrics(args: &[String]) -> Result<MetricArgs, Box<dyn Error>> {
    let parsed = MetricArgs {
        no_upload: args.iter().any(|arg| arg == "--no-upload"),
        no_download: args.iter().any(|arg| arg == "--no-download"),
        ping_only: args.iter().any(|arg| arg == "--ping-only"),
    };
    let count = usize::from(parsed.no_upload)
        + usize::from(parsed.no_download)
        + usize::from(parsed.ping_only);
    if count > 1 {
        return Err("use only one of --no-upload, --no-download, or --ping-only".into());
    }
    Ok(parsed)
}

fn option_value<'a>(args: &'a [String], option: &str) -> Option<&'a str> {
    args.iter()
        .position(|arg| arg == option)
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
}

fn required_option<'a>(args: &'a [String], option: &str) -> Result<&'a str, Box<dyn Error>> {
    option_value(args, option).ok_or_else(|| format!("missing {option}").into())
}

async fn run_and_store(
    settings: &Settings,
    metrics: MetricSelection,
) -> Result<(), Box<dyn Error>> {
    let provider = CloudflareProvider::new(settings.endpoints.clone());
    let result = run_round(&provider, &round_config(settings, metrics)).await;
    let store = open_store()?;
    store.insert_round(&result)?;
    print_round(&result);
    Ok(())
}

async fn continuous(mut settings: Settings, args: ContinuousArgs) -> Result<(), Box<dyn Error>> {
    if let Some(interval) = args.interval {
        settings.interval = interval;
    }
    let scheduler = Scheduler::new(settings.clone());
    println!("continuous mode; press Ctrl+C to stop");
    loop {
        let plan = scheduler.plan_next_round();
        scheduler.record_round_start(SystemTime::now());
        let metrics = intersect_metrics(plan.metrics, metrics_for(&settings, &args.metrics));
        let provider = CloudflareProvider::new(settings.endpoints.clone());
        let mut result = run_round(&provider, &round_config(&settings, metrics)).await;
        if result.skipped_reason.is_none() {
            result.skipped_reason = plan.skip_reason;
        }
        let bytes = result
            .down
            .map_or(0, |throughput| throughput.bytes)
            .saturating_add(result.up.map_or(0, |throughput| throughput.bytes));
        scheduler.record_bytes(bytes);
        let store = open_store()?;
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

async fn ping(settings: &Settings, args: PingArgs) {
    let target = settings.targets.iter().find(|target| {
        target.name.eq_ignore_ascii_case(&args.target)
            || target
                .name
                .to_ascii_lowercase()
                .contains(&args.target.to_ascii_lowercase())
    });
    let Some(target) = target else {
        eprintln!("unknown configured target: {}", args.target);
        return;
    };
    for _ in 0..args.seconds.max(1) {
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

fn export(args: ExportArgs) -> Result<(), Box<dyn Error>> {
    let store = open_store()?;
    let rows =
        store.export_rounds_csv(parse_date(&args.from)?, parse_date(&args.to)?, &args.out)?;
    println!("exported {rows} rounds to {}", args.out.display());
    Ok(())
}

fn round_config(settings: &Settings, metrics: MetricSelection) -> alidade_engine::RoundConfig {
    alidade_engine::RoundConfig {
        metrics,
        idle_ping: IDLE_PING,
        phase_budget: PHASE_BUDGET,
        max_bytes_per_phase: MAX_BYTES_PER_PHASE,
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

fn print_round(result: &alidade_engine::RoundResult) {
    let down = result.down.map_or_else(
        || "skipped".to_string(),
        |t| format!("{:.2} Mbit/s", t.bits_per_sec / 1_000_000.0),
    );
    let up = result.up.map_or_else(
        || "skipped".to_string(),
        |t| format!("{:.2} Mbit/s", t.bits_per_sec / 1_000_000.0),
    );
    let ping = result.ping_idle.map_or_else(
        || "skipped".to_string(),
        |stats| format!("{:.1} ms", stats.avg_ms),
    );
    println!("download: {down}; upload: {up}; idle ping: {ping}");
    if let Some(reason) = &result.skipped_reason {
        println!("skip reason: {reason}");
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
        .ok_or_else(|| "interval needs a unit (s, m, or h)".to_string())?;
    let (number, unit) = value.split_at(split);
    let quantity: u64 = number
        .parse()
        .map_err(|_| "interval must start with a whole number".to_string())?;
    let seconds = match unit {
        "s" => quantity,
        "m" => quantity.saturating_mul(60),
        "h" => quantity.saturating_mul(3600),
        _ => return Err("interval unit must be s, m, or h".to_string()),
    };
    if seconds < 60 {
        return Err("interval must be at least 1 minute".to_string());
    }
    Ok(Duration::from_secs(seconds))
}

fn parse_date(value: &str) -> Result<SystemTime, Box<dyn Error>> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")?;
    let midnight = date
        .and_hms_opt(0, 0, 0)
        .ok_or("date cannot represent midnight")?;
    let seconds = Utc.from_utc_datetime(&midnight).timestamp();
    let seconds = u64::try_from(seconds).map_err(|_| "date is before the Unix epoch")?;
    Ok(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
}
