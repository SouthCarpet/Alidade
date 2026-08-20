//! `SpeedProvider`: the measurement contract, plus a Cloudflare-backed
//! implementation and a fixed-rate mock for testing higher layers (round
//! runner, scheduler) without a network.
//!
//! Every phase is **time-bounded, not byte-bounded** (spec D4): a caller
//! hands over a wall-clock `budget` and gets back whatever was actually
//! moved in that window. A round costs the same wall-clock time on a
//! 50 Mbit/s link and a 1000 Mbit/s link; only the reported `bytes` and
//! `bits_per_sec` differ.
//!
//! `max_bytes` is `Option`, and `None` — no ceiling at all — is the normal
//! case (updated D6). The daily data budget is the only thing allowed to end
//! a phase by byte count, and it is off by default. A ceiling that fires is
//! never silent: the phase comes back `capped`, so a truncated measurement
//! can never be presented as a normal one.

use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context as TaskContext, Poll};
use std::time::{Duration, Instant};

use futures_core::Stream;

use crate::{EngineError, RateLimit};

/// One measured phase: bytes actually moved, how long the measurement clock
/// ran, and the derived rate.
///
/// `duration` is **payload time only**. The clock runs while a response body
/// is being read (download) or while the request body is being pushed
/// (upload); it never runs during connect, TLS, or a request/response header
/// exchange (see the comments in `download`/`upload` for the exact
/// boundaries). A download phase issues several requests (see
/// `DOWNLOAD_CHUNK_BYTES`), so its wall clock is longer than `duration` by
/// the sum of the per-request header round-trips — that overhead is excluded
/// from the rate on purpose (the caller cannot avoid it and did not cause
/// it), but it is not hidden: it is bounded by one round-trip per chunk on a
/// connection that stays warm across the phase.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Throughput {
    pub bits_per_sec: f64,
    pub bytes: u64,
    pub duration: Duration,
    /// The byte ceiling, not the clock, ended this phase. A capped phase is
    /// a truncated measurement — it stopped early, so it carries more TCP
    /// slow start and less steady state than the budget asked for, and its
    /// round is marked capped all the way out to the CSV. With no ceiling
    /// (the default) this is always `false`.
    pub capped: bool,
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
/// `budget` (stop promptly once it elapses) and `max_bytes` when it is
/// `Some` (never move more than that in one phase, and report `capped` when
/// that is what stopped them). `max_bytes: None` means the clock alone ends
/// the phase — the D4/D6 default.
#[async_trait::async_trait]
pub trait SpeedProvider: Send + Sync {
    async fn download(
        &self,
        budget: Duration,
        max_bytes: Option<u64>,
    ) -> Result<Throughput, EngineError>;
    async fn upload(
        &self,
        budget: Duration,
        max_bytes: Option<u64>,
    ) -> Result<Throughput, EngineError>;
}

/// `SpeedProvider` backed by Cloudflare's `__down`/`__up` endpoints (or any
/// endpoint that speaks the same contract: `GET {download_url}?bytes=N`
/// streams N bytes back; `POST {upload_url}` accepts a body and returns
/// 2xx).
pub struct CloudflareProvider {
    client: reqwest::Client,
    cfg: EndpointConfig,
    download_chunk_bytes: u64,
    /// Survives across `download()` calls on this instance so a 429 in one
    /// round stops the next round from hammering the same limit. Callers
    /// that construct a fresh provider per round throw this away.
    rate_limit: Mutex<Option<RateLimitHold>>,
}

impl CloudflareProvider {
    /// Provider with the default download chunk (`DOWNLOAD_CHUNK_BYTES`).
    pub fn new(cfg: EndpointConfig) -> Self {
        Self::with_download_chunk_bytes(cfg, DOWNLOAD_CHUNK_BYTES)
    }

    /// Same, with an explicit per-request download chunk. The chunk is a
    /// tuning knob (see `DOWNLOAD_CHUNK_BYTES`), and tests need one small
    /// enough that a millisecond-scale budget still spans many requests.
    ///
    /// `chunk_bytes` is clamped below `MAX_REQUEST_BYTES`, so no caller can
    /// reintroduce the 403 by asking for a chunk the server refuses.
    pub fn with_download_chunk_bytes(cfg: EndpointConfig, chunk_bytes: u64) -> Self {
        Self {
            client: reqwest::Client::new(),
            cfg,
            download_chunk_bytes: chunk_bytes.clamp(1, MAX_REQUEST_BYTES - 1),
            rate_limit: Mutex::new(None),
        }
    }

    /// One download request for `request_bytes`, read until the body ends or
    /// `phase_deadline` passes. Returns the bytes read and the time the
    /// measurement clock ran for them.
    ///
    /// The clock starts once the response headers are back, so connect, TLS
    /// and this request's header round-trip are outside it. The body read is
    /// bounded by the phase deadline, not by a fresh per-chunk budget, so
    /// chunking cannot stretch the phase past `budget`; only `SEND_GRACE`
    /// can, and only while waiting for a server that has not answered at all.
    async fn download_chunk(
        &self,
        request_bytes: u64,
        phase_deadline: Instant,
    ) -> Result<(u64, Duration), EngineError> {
        let url = format!("{}?bytes={}", self.cfg.download_url, request_bytes);
        let send_budget = phase_deadline.saturating_duration_since(Instant::now()) + SEND_GRACE;
        let mut resp = match tokio::time::timeout(send_budget, self.client.get(&url).send()).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(e)) => return Err(EngineError::Http(e)),
            Err(_elapsed) => return Err(EngineError::Timeout(send_budget)),
        };
        if let Some(err) = self.status_error(&resp) {
            return Err(err);
        }

        let start = Instant::now();
        let mut bytes_read: u64 = 0;
        loop {
            let remaining = phase_deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, resp.chunk()).await {
                Ok(Ok(Some(chunk))) => bytes_read += chunk.len() as u64,
                Ok(Ok(None)) => break, // this chunk's body is done; the caller asks for the next
                Ok(Err(e)) => return Err(EngineError::Http(e)),
                Err(_elapsed) => break, // phase deadline hit mid-read
            }
        }
        Ok((bytes_read, start.elapsed()))
    }

    fn rate_limit_lock(&self) -> std::sync::MutexGuard<'_, Option<RateLimitHold>> {
        self.rate_limit.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// If a previous 429 is still in force, return that condition without
    /// touching the network.
    fn check_rate_limit(&self) -> Result<(), EngineError> {
        let guard = self.rate_limit_lock();
        let Some(hold) = guard.as_ref() else {
            return Ok(());
        };
        let now = Instant::now();
        if now >= hold.not_before {
            return Ok(());
        }
        Err(EngineError::RateLimited(RateLimit {
            retry_after: hold.not_before.saturating_duration_since(now),
            consecutive: hold.consecutive,
        }))
    }

    fn note_rate_limited(&self, retry_after: Option<Duration>) -> EngineError {
        let mut guard = self.rate_limit_lock();
        let consecutive = guard
            .as_ref()
            .map(|hold| hold.consecutive.saturating_add(1))
            .unwrap_or(1);
        let wait = retry_after
            .filter(|wait| !wait.is_zero())
            .unwrap_or_else(|| default_backoff(consecutive));
        *guard = Some(RateLimitHold {
            not_before: Instant::now() + wait,
            consecutive,
        });
        EngineError::RateLimited(RateLimit {
            retry_after: wait,
            consecutive,
        })
    }

    fn clear_rate_limit(&self) {
        *self.rate_limit_lock() = None;
    }

    /// Map a non-success status onto a typed error. 429 becomes
    /// [`EngineError::RateLimited`] and arms backoff; anything else is a
    /// one-shot [`EngineError::Status`].
    fn status_error(&self, resp: &reqwest::Response) -> Option<EngineError> {
        let status = resp.status().as_u16();
        if status == 429 {
            let retry_after = resp
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(parse_retry_after);
            Some(self.note_rate_limited(retry_after))
        } else if resp.status().is_success() {
            None
        } else {
            Some(EngineError::Status(status))
        }
    }
}

/// Upload chunk size: small enough to check the deadline often, large
/// enough not to dominate the measurement with per-chunk overhead.
pub const UPLOAD_CHUNK_BYTES: usize = 64 * 1024;

/// Hard per-request ceiling of Cloudflare's `__down`: a `bytes` value at or
/// above 100,000,000 (decimal) is refused with HTTP 403. This is a request
/// validation limit, not a rate limit — it never recovers by waiting.
///
/// Measured live against `https://speed.cloudflare.com/__down` (GET, this
/// machine, 2026-08-20):
///
/// | `bytes` | result |
/// |---|---|
/// | 1,048,576 | 200 |
/// | 25,000,000 | 200 |
/// | 50,000,000 | 200 |
/// | 67,108,864 | 200 |
/// | 90,000,000 | 200 |
/// | 99,000,000 | 200 |
/// | 100,000,000 | **403** |
/// | 104,857,599 | **403** |
/// | 104,857,600 | **403** |
///
/// So `bytes < 100_000_000` is the contract. The first shipped CLI passed
/// its whole phase ceiling (104,857,600 = 100 MiB) as a single `bytes`
/// value, so every live download was a 403 and download was never measured
/// once against the real service.
pub const MAX_REQUEST_BYTES: u64 = 100_000_000;

/// Bytes asked for per download request. A phase is time-bounded, so it
/// issues as many of these as fit in the budget; the size only decides how
/// many requests that takes, and how much of the budget goes to payload
/// rather than to per-request overhead.
///
/// 90,000,000 — not 25,000,000 — because the request count is what tripped
/// Cloudflare's rate limit. A 10 s phase at 200 Mbit/s moves 250 MB; at
/// 25 MB per request that is ten GETs per round, and eleven consecutive
/// rounds then came back 429. 90 MB was measured live as HTTP 200 (see the
/// table on `MAX_REQUEST_BYTES`); at the same 200 Mbit/s / 10 s point that
/// is three requests, not ten. 99 MB also 200, so 90 MB sits 9 MB inside
/// proven-good territory and 10 MB (10%) below the 403 ceiling — enough
/// because that ceiling is a hard equality at 100,000,000, not a fuzzy
/// "around 100 MB". The constructor still clamps every request below
/// `MAX_REQUEST_BYTES`, so raising this cannot reintroduce the 403.
///
/// A 1 Gbit/s / 10 s phase (1.25 GB) is ~14 requests instead of ~50. One
/// `reqwest::Client` is reused, so the cost between chunks is a header
/// round-trip (~20 ms here), not a TLS handshake: at 1 Gbit/s a 90 MB
/// chunk is ~0.72 s of payload against that ~20 ms.
pub const DOWNLOAD_CHUNK_BYTES: u64 = 90_000_000;

/// First wait when a 429 carries no usable `Retry-After`. Fifteen minutes
/// is longer than the 5-minute cadence that hammered the soak, short enough
/// that a one-off 429 does not eat the next hourly production round, and
/// the first step of a doubling that reaches the ~2 h window the live
/// limit actually occupied.
const RATE_LIMIT_BACKOFF_START: Duration = Duration::from_secs(15 * 60);

/// Cap for the absent-header schedule. The soak's limit was still active
/// an hour after the last 429 and had cleared about two hours after that;
/// waiting longer than the observed recovery is worse than probing once.
const RATE_LIMIT_BACKOFF_CAP: Duration = Duration::from_secs(2 * 60 * 60);

/// `Retry-After` delay-seconds (RFC 9110 §10.2.3). Values above a day are
/// clamped so a garbage integer cannot park download forever.
const RETRY_AFTER_DELAY_CAP: Duration = Duration::from_secs(24 * 60 * 60);

struct RateLimitHold {
    not_before: Instant,
    consecutive: u32,
}

/// Delay-seconds only. The HTTP-date form (`IMF-fixdate`) is unhandled:
/// this crate has no HTTP-date parser, and a date we cannot interpret is
/// treated as an absent header so the built-in schedule still backs off
/// rather than retrying immediately.
fn parse_retry_after(value: &str) -> Option<Duration> {
    let secs: u64 = value.trim().parse().ok()?;
    if secs == 0 {
        return None;
    }
    Some(Duration::from_secs(secs).min(RETRY_AFTER_DELAY_CAP))
}

fn default_backoff(consecutive: u32) -> Duration {
    // 15 m, 30 m, 60 m, 120 m, then the cap.
    let shift = consecutive.saturating_sub(1).min(3);
    RATE_LIMIT_BACKOFF_START
        .saturating_mul(1u32 << shift)
        .min(RATE_LIMIT_BACKOFF_CAP)
}

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
    /// Download for `budget`, never moving more than `max_bytes` in total.
    ///
    /// The phase is **time-bounded**: it issues successive requests of
    /// `download_chunk_bytes` each (always below `MAX_REQUEST_BYTES`, see
    /// C1) until the budget runs out, and reports the bytes accumulated
    /// across all of them. One request could never do this — the server
    /// caps a single request below 100 MB, so a single-request phase can
    /// only measure what fits in one chunk and then stops early, which on a
    /// fast link means measuring a fraction of a second of a 10 s window
    /// (a window dominated by TCP slow start) and leaving the rest of the
    /// round's "ping under load" running under no load at all.
    ///
    /// `max_bytes` is the phase's **total** ceiling — the data-budget guard —
    /// never a per-request size, and normally `None`. When it is `Some` and
    /// it is what ends the phase, the returned `Throughput` says so
    /// (`capped`), because a ceiling that bites also shortens the window: it
    /// was a 100 MiB ceiling against a 10 s budget that ended the phase after
    /// 5.07 s on a 165 Mbit/s link (measured live 2026-08-20) and left the
    /// round's ping-under-load loop sampling an idle link.
    ///
    /// Failure policy: if the **first** request fails there is no
    /// measurement, so the error propagates (never a fake 0). If a **later**
    /// chunk fails once bytes were already measured, the phase returns what
    /// it measured: those bytes really did cross the wire in that time, and
    /// discarding a good 8 s of data because the 9th second broke would
    /// throw away the better measurement of the two.
    async fn download(
        &self,
        budget: Duration,
        max_bytes: Option<u64>,
    ) -> Result<Throughput, EngineError> {
        self.check_rate_limit()?;
        let deadline = Instant::now() + budget;
        let mut bytes_read: u64 = 0;
        // Sum of the per-chunk body-read times. Deliberately excludes each
        // chunk's header round-trip (see `Throughput::duration`).
        let mut measured = Duration::ZERO;
        let mut capped = false;
        let mut hit_rate_limit = false;

        loop {
            let bytes_left = max_bytes.map(|max| max.saturating_sub(bytes_read));
            if bytes_left == Some(0) {
                capped = true;
                break;
            }
            if deadline.saturating_duration_since(Instant::now()).is_zero() {
                break;
            }
            let request_bytes = bytes_left.map_or(self.download_chunk_bytes, |left| {
                self.download_chunk_bytes.min(left)
            });
            match self.download_chunk(request_bytes, deadline).await {
                Ok((bytes, spent)) => {
                    bytes_read += bytes;
                    measured += spent;
                }
                Err(e @ EngineError::RateLimited(_)) if bytes_read == 0 => return Err(e),
                Err(EngineError::RateLimited(_)) => {
                    // Bytes already measured stay; backoff is armed so the
                    // next round does not hammer the same 429.
                    hit_rate_limit = true;
                    break;
                }
                Err(e) if bytes_read == 0 => return Err(e),
                Err(_partial) => break,
            }
        }

        if !hit_rate_limit {
            self.clear_rate_limit();
        }

        Ok(Throughput {
            bytes: bytes_read,
            duration: measured,
            bits_per_sec: bytes_read as f64 * 8.0 / measured.as_secs_f64().max(f64::EPSILON),
            capped,
        })
    }

    /// Upload for `budget`, never moving more than `max_bytes` in total.
    ///
    /// This phase needs no chunked-request treatment (C2): `__up` takes the
    /// payload as the request *body*, and the body is a stream this crate
    /// feeds, so one request already covers the whole budget — the stream
    /// yields `UPLOAD_CHUNK_BYTES` at a time purely to check the deadline
    /// often, and those chunks are frames inside one request, not separate
    /// requests. There is no server-side per-request ceiling like `__down`'s
    /// `bytes` limit to run into, so the phase's measurable rate is bounded
    /// only by `max_bytes / budget` (the data-budget guard, which is the
    /// caller's own choice) and never by a chunk size.
    /// `upload_is_not_capped_by_its_chunk_size` in `tests/provider.rs` holds
    /// that property.
    async fn upload(
        &self,
        budget: Duration,
        max_bytes: Option<u64>,
    ) -> Result<Throughput, EngineError> {
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

        if let Some(err) = self.status_error(&resp) {
            return Err(err);
        }

        let bytes_sent = sent.load(Ordering::Relaxed);
        Ok(Throughput {
            bytes: bytes_sent,
            duration,
            bits_per_sec: bytes_sent as f64 * 8.0 / duration.as_secs_f64().max(f64::EPSILON),
            // The stream stops on `remaining == 0` or on the deadline; having
            // pushed the whole ceiling is exactly the first case.
            capped: max_bytes.is_some_and(|max| bytes_sent >= max),
        })
    }
}

/// Feeds `reqwest::Body::wrap_stream` a sequence of zero-filled chunks,
/// stopping at `remaining == Some(0)` (byte ceiling reached) or `deadline`
/// (time budget elapsed) — whichever comes first. `remaining: None` is the
/// default case: only the deadline stops it. `sent` is read back by the
/// caller after `send().await` completes to learn how much was actually
/// handed to the HTTP layer.
struct UploadStream {
    remaining: Option<u64>,
    deadline: Instant,
    sent: Arc<AtomicU64>,
}

impl Stream for UploadStream {
    type Item = Result<Vec<u8>, std::io::Error>;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.remaining == Some(0) || Instant::now() >= this.deadline {
            return Poll::Ready(None);
        }
        let n = this.remaining.map_or(UPLOAD_CHUNK_BYTES, |left| {
            (UPLOAD_CHUNK_BYTES as u64).min(left) as usize
        });
        if let Some(left) = &mut this.remaining {
            *left -= n as u64;
        }
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

    fn synthesize(rate_bits_per_sec: f64, budget: Duration, max_bytes: Option<u64>) -> Throughput {
        let wanted = ((rate_bits_per_sec / 8.0) * budget.as_secs_f64()).round() as u64;
        let bytes = max_bytes.map_or(wanted, |max| wanted.min(max));
        Throughput {
            bytes,
            duration: budget,
            bits_per_sec: rate_bits_per_sec,
            capped: max_bytes.is_some_and(|max| wanted >= max),
        }
    }
}

#[async_trait::async_trait]
impl SpeedProvider for MockProvider {
    async fn download(
        &self,
        budget: Duration,
        max_bytes: Option<u64>,
    ) -> Result<Throughput, EngineError> {
        if self.fail {
            return Err(EngineError::Other("mock provider configured to fail".to_string()));
        }
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        Ok(Self::synthesize(self.down_bits_per_sec, budget, max_bytes))
    }

    async fn upload(
        &self,
        budget: Duration,
        max_bytes: Option<u64>,
    ) -> Result<Throughput, EngineError> {
        if self.fail {
            return Err(EngineError::Other("mock provider configured to fail".to_string()));
        }
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        Ok(Self::synthesize(self.up_bits_per_sec, budget, max_bytes))
    }
}
