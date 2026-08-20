//! `SpeedProvider`: the measurement contract, plus a Cloudflare-backed
//! implementation and a fixed-rate mock for testing higher layers (round
//! runner, scheduler) without a network.
//!
//! Every phase is **time-bounded, not byte-bounded** (spec D4): a caller
//! hands over a wall-clock `budget` and a `max_bytes` ceiling (politeness /
//! data-budget cap), and gets back whatever was actually moved in that
//! window. A round costs the same wall-clock time on a 50 Mbit/s link and a
//! 1000 Mbit/s link; only the reported `bytes` and `bits_per_sec` differ.

use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};
use std::time::{Duration, Instant};

use futures_core::Stream;

use crate::EngineError;

/// One measured phase: bytes actually moved, how long it took, and the
/// derived rate. `duration` is measured from first payload byte (see the
/// comments in `download`/`upload` for exactly where that boundary is),
/// not from `send()`/connect, so rates are not deflated by TLS/DNS setup.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Throughput {
    pub bits_per_sec: f64,
    pub bytes: u64,
    pub duration: Duration,
}

/// Speed-test endpoint URLs. Configurable on purpose (spec / plan-header
/// constraint): logic never hardcodes a URL at the call site, it always
/// reads `EndpointConfig`. `Default` carries Cloudflare's own endpoints
/// (confirmed from Cloudflare's speed-test repo, no documented rate limit;
/// see `vault/knowledge/raw/raw_speedtest_targets_2026-08.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointConfig {
    pub download_url: String,
    pub upload_url: String,
}

impl Default for EndpointConfig {
    fn default() -> Self {
        Self {
            download_url: "https://speed.cloudflare.com/__down".to_string(),
            upload_url: "https://speed.cloudflare.com/__up".to_string(),
        }
    }
}

/// A source of download/upload measurements. Implementations must honor
/// `budget` (stop promptly once it elapses) and `max_bytes` (never move
/// more than that in one phase) — both are politeness/data-budget limits,
/// not targets to race toward.
#[async_trait::async_trait]
pub trait SpeedProvider: Send + Sync {
    async fn download(&self, budget: Duration, max_bytes: u64) -> Result<Throughput, EngineError>;
    async fn upload(&self, budget: Duration, max_bytes: u64) -> Result<Throughput, EngineError>;
}

/// `SpeedProvider` backed by Cloudflare's `__down`/`__up` endpoints (or any
/// endpoint that speaks the same contract: `GET {download_url}?bytes=N`
/// streams N bytes back; `POST {upload_url}` accepts a body and returns
/// 2xx).
pub struct CloudflareProvider {
    client: reqwest::Client,
    cfg: EndpointConfig,
}

impl CloudflareProvider {
    pub fn new(cfg: EndpointConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            cfg,
        }
    }
}

/// Upload chunk size: small enough to check the deadline often, large
/// enough not to dominate the measurement with per-chunk overhead.
const UPLOAD_CHUNK_BYTES: usize = 64 * 1024;

/// Extra time allowed, beyond the phase `budget`, for the server to send
/// its *first* response bytes (download) or to finish acknowledging a
/// fully-sent request body (upload) — server "think time", not transfer
/// time. `budget` bounds how long we spend moving bytes once the exchange
/// is under way; `SEND_GRACE` bounds how long we'll wait for that exchange
/// to start responding at all. Without this, an unresponsive server hangs
/// `send()` forever — `budget` alone only ever bounded the body-read loop
/// (download) or was invisible to the final response wait (upload).
const SEND_GRACE: Duration = Duration::from_secs(5);

#[async_trait::async_trait]
impl SpeedProvider for CloudflareProvider {
    async fn download(&self, budget: Duration, max_bytes: u64) -> Result<Throughput, EngineError> {
        let url = format!("{}?bytes={}", self.cfg.download_url, max_bytes);
        let send_budget = budget + SEND_GRACE;
        let mut resp = match tokio::time::timeout(send_budget, self.client.get(&url).send()).await
        {
            Ok(Ok(resp)) => resp,
            Ok(Err(e)) => return Err(EngineError::Http(e)),
            Err(_elapsed) => return Err(EngineError::Timeout(send_budget)),
        };
        if !resp.status().is_success() {
            return Err(EngineError::Status(resp.status().as_u16()));
        }

        // Start the clock once headers are back: connect + TLS + request-send
        // already happened during `send().await` above, so this measures wire
        // time for the response body only, not connection setup.
        let start = Instant::now();
        let deadline = start + budget;
        let mut bytes_read: u64 = 0;

        while bytes_read < max_bytes {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, resp.chunk()).await {
                Ok(Ok(Some(chunk))) => {
                    bytes_read += chunk.len() as u64;
                }
                Ok(Ok(None)) => break, // body exhausted before budget/cap
                Ok(Err(e)) => return Err(EngineError::Http(e)),
                Err(_elapsed) => break, // budget hit mid-read
            }
        }

        let duration = start.elapsed();
        Ok(Throughput {
            bytes: bytes_read,
            duration,
            bits_per_sec: bytes_read as f64 * 8.0 / duration.as_secs_f64().max(f64::EPSILON),
        })
    }

    async fn upload(&self, budget: Duration, max_bytes: u64) -> Result<Throughput, EngineError> {
        let deadline = Instant::now() + budget;
        let sent = Arc::new(AtomicU64::new(0));
        let stream = UploadStream {
            remaining: max_bytes,
            deadline,
            sent: sent.clone(),
        };
        let body = reqwest::Body::wrap_stream(stream);

        // Start the clock right before handing the body to the client. This
        // still includes request-line/header write, which we can't isolate
        // without lower-level socket hooks; in the real round (spec D4) an
        // idle ping runs first, so the connection is already warm by the time
        // upload starts and connect/TLS cost is not on this path in practice.
        // Concretely: `download()` runs immediately before `upload()` in that
        // sequence and already established the connection to this same
        // client/host, so this clock excludes only that already-hidden cost,
        // never a cold connect in the real flow.
        let start = Instant::now();
        let send_budget = budget + SEND_GRACE;
        let resp = match tokio::time::timeout(
            send_budget,
            self.client.post(&self.cfg.upload_url).body(body).send(),
        )
        .await
        {
            Ok(Ok(resp)) => resp,
            Ok(Err(e)) => return Err(EngineError::Http(e)),
            Err(_elapsed) => return Err(EngineError::Timeout(send_budget)),
        };
        let duration = start.elapsed();

        if !resp.status().is_success() {
            return Err(EngineError::Status(resp.status().as_u16()));
        }

        let bytes_sent = sent.load(Ordering::Relaxed);
        Ok(Throughput {
            bytes: bytes_sent,
            duration,
            bits_per_sec: bytes_sent as f64 * 8.0 / duration.as_secs_f64().max(f64::EPSILON),
        })
    }
}

/// Feeds `reqwest::Body::wrap_stream` a sequence of zero-filled chunks,
/// stopping at `remaining == 0` (byte cap reached) or `deadline` (time
/// budget elapsed) — whichever comes first. `sent` is read back by the
/// caller after `send().await` completes to learn how much was actually
/// handed to the HTTP layer.
struct UploadStream {
    remaining: u64,
    deadline: Instant,
    sent: Arc<AtomicU64>,
}

impl Stream for UploadStream {
    type Item = Result<Vec<u8>, std::io::Error>;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.remaining == 0 || Instant::now() >= this.deadline {
            return Poll::Ready(None);
        }
        let n = (UPLOAD_CHUNK_BYTES as u64).min(this.remaining) as usize;
        this.remaining -= n as u64;
        this.sent.fetch_add(n as u64, Ordering::Relaxed);
        Poll::Ready(Some(Ok(vec![0u8; n])))
    }
}

/// Fixed-rate `SpeedProvider` for testing the round runner and scheduler
/// without a network. Returns immediately — it never actually waits out
/// `budget` — so higher-layer tests stay fast.
pub struct MockProvider {
    down_bits_per_sec: f64,
    up_bits_per_sec: f64,
    fail: bool,
    delay: Duration,
}

impl MockProvider {
    /// Reports `bits_per_sec` for both download and upload, synthesized
    /// instantly (no sleep).
    pub fn new(bits_per_sec: f64) -> Self {
        Self {
            down_bits_per_sec: bits_per_sec,
            up_bits_per_sec: bits_per_sec,
            fail: false,
            delay: Duration::ZERO,
        }
    }

    /// A provider whose `download`/`upload` both return `Err` — for testing
    /// that a provider failure is recorded and skipped, never abandons the
    /// caller (round runner, Task 4).
    pub fn failing() -> Self {
        Self {
            down_bits_per_sec: 0.0,
            up_bits_per_sec: 0.0,
            fail: true,
            delay: Duration::ZERO,
        }
    }

    /// Like `new`, but each `download`/`upload` call sleeps `delay` before
    /// returning its (still instantly synthesized) throughput. `new`'s
    /// provider resolves in effectively zero wall-clock time, which hides a
    /// `RoundRunner` regression that serializes ping and provider instead of
    /// running them concurrently (spec D4) — every round test would still
    /// pass. This variant gives the provider phase real duration so that
    /// property is provable (see `round.rs` tests, Task 4b).
    pub fn with_delay(bits_per_sec: f64, delay: Duration) -> Self {
        Self {
            down_bits_per_sec: bits_per_sec,
            up_bits_per_sec: bits_per_sec,
            fail: false,
            delay,
        }
    }

    fn synthesize(rate_bits_per_sec: f64, budget: Duration, max_bytes: u64) -> Throughput {
        let bytes = ((rate_bits_per_sec / 8.0) * budget.as_secs_f64()).round() as u64;
        let bytes = bytes.min(max_bytes);
        Throughput {
            bytes,
            duration: budget,
            bits_per_sec: rate_bits_per_sec,
        }
    }
}

#[async_trait::async_trait]
impl SpeedProvider for MockProvider {
    async fn download(&self, budget: Duration, max_bytes: u64) -> Result<Throughput, EngineError> {
        if self.fail {
            return Err(EngineError::Other("mock provider configured to fail".to_string()));
        }
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        Ok(Self::synthesize(self.down_bits_per_sec, budget, max_bytes))
    }

    async fn upload(&self, budget: Duration, max_bytes: u64) -> Result<Throughput, EngineError> {
        if self.fail {
            return Err(EngineError::Other("mock provider configured to fail".to_string()));
        }
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        Ok(Self::synthesize(self.up_bits_per_sec, budget, max_bytes))
    }
}
