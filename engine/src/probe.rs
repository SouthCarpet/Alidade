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
/// (including losses); `loss_pct` is always present, because loss is the
/// measurement.
///
/// The timing fields are computed over the successful samples only and are
/// `None` when there is nothing to compute them from. A target that answered
/// nothing has **no RTT** — reporting `0.0` there would make a dead target
/// indistinguishable from a perfect one and would drag every average that
/// touches it towards zero. `jitter_ms` is a difference between consecutive
/// successes, so it needs two of them: one successful sample yields `Some`
/// avg/min/max and `None` jitter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PingStats {
    pub avg_ms: Option<f64>,
    pub min_ms: Option<f64>,
    pub max_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
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

/// Resolves `host:port` first, then times only the connect itself — so a
/// hostname target's reported RTT is connect time, not resolve+connect
/// time. The two steps together still respect the caller's `timeout`: any
/// budget DNS resolution spends comes out of what's left for connecting.
async fn tcp_connect_rtt(host: &str, port: u16, timeout: Duration) -> Option<Duration> {
    let deadline = Instant::now() + timeout;

    let mut addrs = match tokio::time::timeout(timeout, tokio::net::lookup_host((host, port))).await
    {
        Ok(Ok(addrs)) => addrs,
        Ok(Err(_)) => return None, // resolution failed (NXDOMAIN etc.)
        Err(_) => return None,     // resolution itself ran past the deadline
    };
    let addr = addrs.next()?; // host resolved to nothing usable

    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return None; // whole budget already spent on resolution
    }

    let start = Instant::now();
    match tokio::time::timeout(remaining, TcpStream::connect(addr)).await {
        Ok(Ok(_stream)) => Some(start.elapsed()),
        Ok(Err(_)) => None, // connection refused / unreachable
        Err(_) => None,     // timed out
    }
}

/// Mean absolute difference between consecutive successful samples, in
/// order of appearance (losses are skipped, not treated as a gap value).
/// `None` with fewer than two successes — no pair to diff, and `0.0` there
/// would read as "perfectly stable".
fn jitter_ms(successes_ms: &[f64]) -> Option<f64> {
    if successes_ms.len() < 2 {
        return None;
    }
    let diffs: f64 = successes_ms.windows(2).map(|w| (w[1] - w[0]).abs()).sum();
    Some(diffs / (successes_ms.len() - 1) as f64)
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
        // Nothing answered: there is no RTT to report. The loss stays.
        return PingStats {
            avg_ms: None,
            min_ms: None,
            max_ms: None,
            jitter_ms: None,
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
        avg_ms: Some(avg_ms),
        min_ms: Some(min_ms),
        max_ms: Some(max_ms),
        jitter_ms: jitter_ms(&successes_ms),
        loss_pct,
        sent,
    }
}

/// Resolve a probe host to an IPv4 address. A literal IP short-circuits
/// here (no resolver call); anything else is handed to `resolve` — the
/// blocking step callers on the ICMP path already run inside
/// `spawn_blocking`. Generic over the resolver so tests can inject one that
/// simulates a slow or hanging DNS lookup (see `icmp_rtt_bounded` below);
/// production always calls this with `dns_lookup_ipv4`, the real platform
/// resolver.
fn resolve_ipv4_with(host: &str, resolve: impl FnOnce(&str) -> Option<Ipv4Addr>) -> Option<Ipv4Addr> {
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        return Some(ip);
    }
    resolve(host)
}

/// Blocking DNS lookup via `ToSocketAddrs` — the real resolver production
/// code plugs into `resolve_ipv4_with` (a literal IP never reaches this).
fn dns_lookup_ipv4(host: &str) -> Option<Ipv4Addr> {
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
/// `IcmpSendEcho` itself is bounded by `timeout_ms`, but the resolve step
/// inside `icmp_rtt_blocking` (`resolve_ipv4`, a blocking DNS call) is not —
/// a slow or dead resolver could otherwise block the OS thread indefinitely
/// while this `async fn` waits on it forever. Wrapping the whole
/// `spawn_blocking` in `tokio::time::timeout` bounds the *caller's* view of
/// this call by `timeout` regardless of what the blocking thread is doing;
/// on elapse we return a loss and abandon that thread (it keeps running to
/// completion in the blocking pool, but nothing is left waiting on it).
#[cfg(windows)]
async fn icmp_rtt(host: &str, timeout: Duration) -> Option<Duration> {
    icmp_rtt_bounded(host, timeout, dns_lookup_ipv4).await
}

/// Shared by `icmp_rtt` (production, always called with `dns_lookup_ipv4`)
/// and a test below (called with a resolver that sleeps past `timeout`).
/// Generic over the resolver so the timeout-bound property documented above
/// is provable without touching the network: swap in a resolver that hangs
/// and confirm the call still returns (as a loss) within `timeout`, not
/// after the hang.
#[cfg(windows)]
async fn icmp_rtt_bounded(
    host: &str,
    timeout: Duration,
    resolve: impl FnOnce(&str) -> Option<Ipv4Addr> + Send + 'static,
) -> Option<Duration> {
    let host = host.to_string();
    let timeout_ms = u32::try_from(timeout.as_millis())
        .unwrap_or(u32::MAX)
        .max(1);
    match tokio::time::timeout(
        timeout,
        tokio::task::spawn_blocking(move || icmp_rtt_blocking_with(&host, timeout_ms, resolve)),
    )
    .await
    {
        Ok(Ok(rtt)) => rtt,
        Ok(Err(_join_error)) => None, // blocking task panicked
        Err(_elapsed) => None,        // resolution or ICMP call ran past the deadline
    }
}

#[cfg(not(windows))]
async fn icmp_rtt(_host: &str, _timeout: Duration) -> Option<Duration> {
    // ICMP is Windows-first per spec D10 — non-Windows builds stay
    // compiling (workspace constraint) but always report loss here.
    None
}

#[cfg(windows)]
fn icmp_rtt_blocking_with(
    host: &str,
    timeout_ms: u32,
    resolve: impl FnOnce(&str) -> Option<Ipv4Addr>,
) -> Option<Duration> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::NetworkManagement::IpHelper::{
        IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho, ICMP_ECHO_REPLY,
    };

    let dest = resolve_ipv4_with(host, resolve)?;
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
        // Back the buffer with `u64` words, not bytes: `IcmpSendEcho` writes
        // an `ICMP_ECHO_REPLY` (contains pointer-sized fields) into it, and
        // a `Vec<u8>` only guarantees byte alignment for that write.
        let mut reply_buf: Vec<u64> = vec![0u64; reply_capacity.div_ceil(8)];
        let reply_buf_bytes = (reply_buf.len() * 8) as u32;

        let replies = IcmpSendEcho(
            guard.0,
            dest_addr,
            request_data.as_ptr() as *const core::ffi::c_void,
            request_data.len() as u16,
            None,
            reply_buf.as_mut_ptr() as *mut core::ffi::c_void,
            reply_buf_bytes,
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
        // A literal IP must short-circuit before the resolver ever runs —
        // proven by handing it a resolver that always fails: if the literal
        // path were broken, this would resolve to `None` instead.
        assert_eq!(
            resolve_ipv4_with("127.0.0.1", |_| None),
            Some(Ipv4Addr::new(127, 0, 0, 1))
        );
    }

    /// `icmp_probe_to_an_unresolvable_host_is_a_bounded_loss` in
    /// `tests/probe.rs` uses `.invalid`, whose local resolution fails in
    /// ~30 ms — it can't tell a `tokio::time::timeout` wrapper from no
    /// wrapper at all. This test closes that gap directly: it injects a
    /// resolver that sleeps well past `timeout` before returning, so it can
    /// only pass if something actually bounds the wait. Mutation-checked:
    /// removing the `tokio::time::timeout(...)` around `spawn_blocking` in
    /// `icmp_rtt_bounded` makes this call block for the full 2 s hang and
    /// fails the upper-bound assertion (see task-4b-report.md).
    #[cfg(windows)]
    #[tokio::test]
    async fn icmp_rtt_is_bounded_by_timeout_even_when_dns_resolution_hangs() {
        let timeout = Duration::from_millis(200);
        let hang = Duration::from_secs(2);

        let start = Instant::now();
        let rtt = icmp_rtt_bounded("host-does-not-matter.example", timeout, move |_host| {
            std::thread::sleep(hang);
            None
        })
        .await;
        let elapsed = start.elapsed();

        assert!(
            rtt.is_none(),
            "a resolver that never returns in time must be a loss"
        );
        assert!(
            elapsed >= timeout,
            "elapsed {elapsed:?} was suspiciously faster than the timeout {timeout:?}"
        );
        assert!(
            elapsed < timeout + Duration::from_millis(500),
            "elapsed {elapsed:?} was not bounded by timeout {timeout:?} — was the timeout wrapper removed?"
        );
    }
}
