//! Probes: `Icmp` (ping, no admin rights needed on Windows) and
//! `TcpConnect` (fallback for hosts that ignore ICMP — many game servers
//! do, per `vault/knowledge/raw/raw_speedtest_targets_2026-08.md`).
//!
//! Targets are DATA, not code (spec D3): this module never hardcodes a
//! host — every `Probe` is built by the caller from the settings file.

use std::net::{IpAddr, Ipv4Addr, ToSocketAddrs};
use std::time::{Duration, Instant, SystemTime};

use tokio::net::TcpStream;

/// One configured probe target. `host` may be a literal IP or a hostname
/// (resolved at probe time, not at construction, so DNS changes are picked
/// up between rounds).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    Icmp { host: String },
    TcpConnect { host: String, port: u16 },
}

/// One measurement: `rtt: None` means the probe was lost (timeout, refused,
/// unreachable) — never a panic, never an `Err`. Loss is data, not failure.
#[derive(Debug, Clone, Copy)]
pub struct PingSample {
    pub at: SystemTime,
    pub rtt: Option<Duration>,
}

/// Aggregate stats over a run of samples. `sent` counts every sample
/// (including losses); the timing fields (`avg_ms`/`min_ms`/`max_ms`/
/// `jitter_ms`) are computed only over the successful ones and are `0.0`
/// when nothing succeeded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PingStats {
    pub avg_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub jitter_ms: f64,
    pub loss_pct: f64,
    pub sent: u32,
}

/// Run one probe and return the sample. Never errors: every failure mode
/// (timeout, connection refused, DNS failure, ICMP unreachable) collapses
/// to `rtt: None` so callers can treat "lost" uniformly across probe kinds.
pub async fn probe_once(p: &Probe, timeout: Duration) -> PingSample {
    let at = SystemTime::now();
    let rtt = match p {
        Probe::TcpConnect { host, port } => tcp_connect_rtt(host, *port, timeout).await,
        Probe::Icmp { host } => icmp_rtt(host, timeout).await,
    };
    PingSample { at, rtt }
}

async fn tcp_connect_rtt(host: &str, port: u16, timeout: Duration) -> Option<Duration> {
    let start = Instant::now();
    match tokio::time::timeout(timeout, TcpStream::connect((host, port))).await {
        Ok(Ok(_stream)) => Some(start.elapsed()),
        Ok(Err(_)) => None, // connection refused / unreachable / DNS failure
        Err(_) => None,     // timed out
    }
}

/// Mean absolute difference between consecutive successful samples, in
/// order of appearance (losses are skipped, not treated as a gap value).
/// `0.0` with fewer than two successes — no pair to diff.
fn jitter_ms(successes_ms: &[f64]) -> f64 {
    if successes_ms.len() < 2 {
        return 0.0;
    }
    let diffs: f64 = successes_ms.windows(2).map(|w| (w[1] - w[0]).abs()).sum();
    diffs / (successes_ms.len() - 1) as f64
}

pub fn stats(samples: &[PingSample]) -> PingStats {
    let sent = samples.len() as u32;
    let successes_ms: Vec<f64> = samples
        .iter()
        .filter_map(|s| s.rtt.map(|d| d.as_secs_f64() * 1000.0))
        .collect();

    let lost = sent as usize - successes_ms.len();
    let loss_pct = if sent == 0 {
        0.0
    } else {
        lost as f64 / sent as f64 * 100.0
    };

    if successes_ms.is_empty() {
        return PingStats {
            avg_ms: 0.0,
            min_ms: 0.0,
            max_ms: 0.0,
            jitter_ms: 0.0,
            loss_pct,
            sent,
        };
    }

    let sum: f64 = successes_ms.iter().sum();
    let avg_ms = sum / successes_ms.len() as f64;
    let min_ms = successes_ms.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_ms = successes_ms
        .iter()
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max);

    PingStats {
        avg_ms,
        min_ms,
        max_ms,
        jitter_ms: jitter_ms(&successes_ms),
        loss_pct,
        sent,
    }
}

/// Resolve a probe host to an IPv4 address. Accepts a literal IP directly;
/// otherwise resolves via the platform resolver (blocking — callers on the
/// ICMP path already run inside `spawn_blocking`).
fn resolve_ipv4(host: &str) -> Option<Ipv4Addr> {
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        return Some(ip);
    }
    (host, 0u16)
        .to_socket_addrs()
        .ok()?
        .find_map(|addr| match addr.ip() {
            IpAddr::V4(v4) => Some(v4),
            IpAddr::V6(_) => None,
        })
}

// ICMP is Windows-first per spec D10 (the app itself is Windows-first; the
// engine crate stays cross-platform-compiling so it isn't the reason a
// future port breaks). No-admin-rights ping via the IP Helper API
// (`IcmpCreateFile`/`IcmpSendEcho`), the same mechanism the `winping` crate
// wraps — used directly here via `windows` to match this crate's existing
// dependency style (see `provider.rs`: official, actively maintained
// bindings over a thin third-party wrapper).
#[cfg(windows)]
async fn icmp_rtt(host: &str, timeout: Duration) -> Option<Duration> {
    let host = host.to_string();
    let timeout_ms = u32::try_from(timeout.as_millis())
        .unwrap_or(u32::MAX)
        .max(1);
    tokio::task::spawn_blocking(move || icmp_rtt_blocking(&host, timeout_ms))
        .await
        .ok()
        .flatten()
}

#[cfg(not(windows))]
async fn icmp_rtt(_host: &str, _timeout: Duration) -> Option<Duration> {
    // ICMP is Windows-first per spec D10 — non-Windows builds stay
    // compiling (workspace constraint) but always report loss here.
    None
}

#[cfg(windows)]
fn icmp_rtt_blocking(host: &str, timeout_ms: u32) -> Option<Duration> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::NetworkManagement::IpHelper::{
        IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho, ICMP_ECHO_REPLY,
    };

    let dest = resolve_ipv4(host)?;
    // IcmpSendEcho's IPAddr is a raw 4-byte value in the same left-to-right
    // octet order as the dotted address; on a little-endian host that is
    // exactly what `from_ne_bytes` over `octets()` produces (the standard
    // trick for this API — see e.g. the `winping` crate's own conversion).
    let dest_addr: u32 = u32::from_ne_bytes(dest.octets());

    // RAII guard so the handle is closed on every return path, including
    // the early `?`s below.
    struct IcmpHandle(HANDLE);
    impl Drop for IcmpHandle {
        fn drop(&mut self) {
            unsafe {
                let _ = IcmpCloseHandle(self.0);
            }
        }
    }

    unsafe {
        let handle = IcmpCreateFile().ok()?;
        let guard = IcmpHandle(handle);

        let request_data = b"alidade";
        let reply_capacity = std::mem::size_of::<ICMP_ECHO_REPLY>() + request_data.len() + 8;
        let mut reply_buf = vec![0u8; reply_capacity];

        let replies = IcmpSendEcho(
            guard.0,
            dest_addr,
            request_data.as_ptr() as *const core::ffi::c_void,
            request_data.len() as u16,
            None,
            reply_buf.as_mut_ptr() as *mut core::ffi::c_void,
            reply_buf.len() as u32,
            timeout_ms,
        );

        if replies == 0 {
            return None;
        }

        let reply = &*(reply_buf.as_ptr() as *const ICMP_ECHO_REPLY);
        // IP_STATUS 0 == IP_SUCCESS; anything else (unreachable, TTL
        // expired, ...) is a loss for our purposes.
        if reply.Status != 0 {
            return None;
        }
        Some(Duration::from_millis(reply.RoundTripTime as u64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_ipv4_accepts_a_literal_loopback_address() {
        assert_eq!(resolve_ipv4("127.0.0.1"), Some(Ipv4Addr::new(127, 0, 0, 1)));
    }
}
