//! Deterministic demo-data seeder for Task 7 screenshots (plan 052, UI Task
//! 7 pre-work B2).
//!
//! Writes a fresh store at the given path: 60 rounds spread evenly over the
//! last 24 h (download 80-120 Mbit/s, upload 20-30 Mbit/s, idle ping
//! 8-15 ms, under-load ping 20-60 ms, an occasional real loss), plus one
//! hour of per-minute ping samples for two targets — one healthy, one
//! failing. Every value comes from a fixed-seed generator (`Rng` below), so
//! the same command always writes the same numbers: no `rand::random()`
//! anywhere in this file (spec B2: "no randomness without a fixed seed").
//!
//! Usage: `seed_demo <path-to-db>`

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, SystemTime};

use alidade_engine::{
    LoadWindow, PingSample, PingStats, Probe, RoundKind, RoundResult, Settings, TargetSpec,
    Throughput,
};
use alidade_store::Store;

const ROUND_COUNT: u32 = 60;
const WINDOW_SECS: u64 = 24 * 60 * 60;
const PING_MINUTES: u64 = 60;

/// A tiny fixed-seed xorshift64 generator. Deterministic across runs and
/// platforms; small enough that this demo fixture needs no `rand`
/// dependency.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// A value uniformly in `[lo, hi]`.
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        let fraction = (self.next_u64() % 1_000_000) as f64 / 1_000_000.0;
        lo + fraction * (hi - lo)
    }
}

fn main() -> ExitCode {
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: seed_demo <path-to-db>");
        return ExitCode::FAILURE;
    };
    let path = PathBuf::from(path);

    if let Some(parent) = path.parent() {
        if let Err(err) = std::fs::create_dir_all(parent) {
            eprintln!("could not create {}: {err}", parent.display());
            return ExitCode::FAILURE;
        }
    }
    // A leftover demo DB from an earlier run must not leak stale rounds into
    // today's screenshots.
    let _ = std::fs::remove_file(&path);

    let store = match Store::open(&path) {
        Ok(store) => store,
        Err(err) => {
            eprintln!("could not open {}: {err}", path.display());
            return ExitCode::FAILURE;
        }
    };

    let mut rng = Rng::new(0x0041_4c49_4441_4445); // fixed seed, spells "ALIDADE"-ish in hex
    let now = SystemTime::now();
    let start = now
        .checked_sub(Duration::from_secs(WINDOW_SECS))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let step = Duration::from_secs(WINDOW_SECS / u64::from(ROUND_COUNT));

    for i in 0..ROUND_COUNT {
        let started_at = start + step * i;
        let result = demo_round(&mut rng, started_at);
        if let Err(err) = store.insert_round(&result) {
            eprintln!("round {i}: {err}");
            return ExitCode::FAILURE;
        }
    }

    seed_ping_targets(&mut rng, &store, now);

    // F1 (plan 052 Task 7 fix wave): the Ping monitor reads its target list
    // from settings, not from the store — a settings file that never named
    // these two targets was the actual cause of "No enabled targets"
    // showing while the raw samples above already existed. Writing it next
    // to the DB, rather than adding a second CLI argument, keeps the two
    // always paired for a screenshot run (`--db` and `--settings` then just
    // point at sibling files).
    let settings_path = path.with_file_name("settings.toml");
    if let Err(err) = demo_settings().save(&settings_path) {
        eprintln!(
            "could not write demo settings {}: {err}",
            settings_path.display()
        );
        return ExitCode::FAILURE;
    }

    println!("seeded {ROUND_COUNT} rounds into {}", path.display());
    println!("wrote demo settings to {}", settings_path.display());
    ExitCode::SUCCESS
}

/// One demo round: realistic values across the requested bands, with roughly
/// one round in fifteen showing a real, nonzero loss so the loss column is
/// not a flat zero in every screenshot.
fn demo_round(rng: &mut Rng, started_at: SystemTime) -> RoundResult {
    let down_mbit = rng.range(80.0, 120.0);
    let up_mbit = rng.range(20.0, 30.0);
    let idle_ms = rng.range(8.0, 15.0);
    let load_ms = rng.range(20.0, 60.0);
    let lossy = rng.next_u64().is_multiple_of(15);
    let loss_pct = if lossy { rng.range(2.0, 8.0) } else { 0.0 };

    let phase_duration = Duration::from_secs(10);
    let down = Throughput {
        bits_per_sec: down_mbit * 1_000_000.0,
        bytes: (down_mbit * 1_000_000.0 / 8.0 * phase_duration.as_secs_f64()) as u64,
        duration: phase_duration,
        capped: false,
    };
    let up = Throughput {
        bits_per_sec: up_mbit * 1_000_000.0,
        bytes: (up_mbit * 1_000_000.0 / 8.0 * phase_duration.as_secs_f64()) as u64,
        duration: phase_duration,
        capped: false,
    };
    let ping_idle = PingStats {
        avg_ms: Some(idle_ms),
        min_ms: Some(idle_ms * 0.8),
        max_ms: Some(idle_ms * 1.2),
        jitter_ms: Some(1.5),
        loss_pct: 0.0,
        sent: 10,
    };
    let ping_under_load = PingStats {
        avg_ms: Some(load_ms),
        min_ms: Some(load_ms * 0.8),
        max_ms: Some(load_ms * 1.2),
        jitter_ms: Some(3.0),
        loss_pct,
        sent: 10,
    };
    let window = LoadWindow {
        duration: phase_duration,
        ping_samples: 9,
        bytes: down.bytes,
    };

    RoundResult {
        started_at,
        kind: RoundKind::Full,
        down: Some(down),
        up: Some(up),
        ping_idle: Some(ping_idle),
        ping_down: Some(ping_under_load),
        ping_up: Some(ping_under_load),
        down_load: Some(window),
        up_load: Some(window),
        capped: false,
        skipped_reason: None,
    }
}

/// Ping-monitor demo targets: name, and whether it is the failing one.
/// Shared between the raw samples below and `demo_settings`'s target list,
/// so the two can never name mismatched targets (F1, plan 052 Task 7 fix
/// wave: that exact mismatch — a settings file whose targets did not match
/// these names — was the "No enabled targets" root cause).
const PING_TARGETS: [(&str, bool); 2] = [("cloudflare-anycast", false), ("game-server-eu", true)];

/// One hour of per-minute samples for two targets: `cloudflare-anycast`
/// healthy throughout, `game-server-eu` failing two samples out of three —
/// so the Ping monitor and History screenshots both show a live and a
/// failing target, per the brief.
fn seed_ping_targets(rng: &mut Rng, store: &Store, now: SystemTime) {
    for (name, failing) in PING_TARGETS {
        let mut samples = Vec::with_capacity(PING_MINUTES as usize);
        for minute in 0..PING_MINUTES {
            let ago = Duration::from_secs((PING_MINUTES - minute) * 60);
            let at = now.checked_sub(ago).unwrap_or(SystemTime::UNIX_EPOCH);
            let rtt = if failing {
                if minute.is_multiple_of(3) {
                    Some(Duration::from_secs_f64(rng.range(180.0, 260.0) / 1000.0))
                } else {
                    None
                }
            } else {
                Some(Duration::from_secs_f64(rng.range(10.0, 18.0) / 1000.0))
            };
            samples.push(PingSample { at, rtt });
        }
        let _ = store.insert_ping_samples(name, &samples);
    }
}

/// The settings file this seeder writes next to the demo DB (F1, plan 052
/// Task 7 fix wave): the same two targets `PING_TARGETS` names, both
/// enabled, so the Ping monitor lists them and the store already has
/// history for both. `game-server-eu` (the failing one) is marked
/// unverified, the same badge a real community-sourced preset gets.
fn demo_settings() -> Settings {
    let targets = PING_TARGETS
        .iter()
        .map(|(name, failing)| TargetSpec {
            name: (*name).to_string(),
            probe: demo_probe(name),
            verified: !failing,
            enabled: true,
        })
        .collect();
    Settings {
        targets,
        ..Settings::default()
    }
}

/// A plausible probe per demo target. The addresses are not dialled during
/// a screenshot run (the app never presses Start on its own), so a
/// documentation/reserved address for the fictitious `game-server-eu` is
/// fine here.
fn demo_probe(name: &str) -> Probe {
    match name {
        "cloudflare-anycast" => Probe::Icmp {
            host: "1.1.1.1".to_string(),
        },
        _ => Probe::TcpConnect {
            host: "203.0.113.10".to_string(),
            port: 443,
        },
    }
}
