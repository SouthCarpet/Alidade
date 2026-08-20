use alidade_engine::{
    CloudflareProvider, EndpointConfig, EngineError, SpeedProvider, MAX_REQUEST_BYTES,
    UPLOAD_CHUNK_BYTES,
};
use std::time::{Duration, Instant};
use wiremock::{
    matchers::{method, path},
    Match, Mock, MockServer, Request, Respond, ResponseTemplate,
};

fn endpoints(server: &MockServer) -> EndpointConfig {
    EndpointConfig {
        download_url: format!("{}/__down", server.uri()),
        upload_url: format!("{}/__up", server.uri()),
    }
}

fn requested_bytes(request: &Request) -> Option<u64> {
    request
        .url
        .query_pairs()
        .find(|(key, _)| key == "bytes")?
        .1
        .parse()
        .ok()
}

/// The live `__down` contract, as measured on 2026-08-20: serve exactly the
/// requested number of bytes, and refuse a `bytes` value at or above
/// 100,000,000 with 403. The unit tests before this task used a mock that
/// answered ANY `bytes` value, which is why none of them could see that the
/// shipped CLI asked for 104,857,600 and got a 403 every single time.
struct LikeCloudflareDown;

impl Respond for LikeCloudflareDown {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        match requested_bytes(request) {
            Some(bytes) if bytes >= MAX_REQUEST_BYTES => ResponseTemplate::new(403),
            Some(bytes) => ResponseTemplate::new(200).set_body_bytes(vec![7u8; bytes as usize]),
            None => ResponseTemplate::new(400),
        }
    }
}

/// Matches a `bytes` query parameter at or above the ceiling — used to prove
/// a request that would 403 is never even issued.
struct BytesAtLeast(u64);

impl Match for BytesAtLeast {
    fn matches(&self, request: &Request) -> bool {
        requested_bytes(request).is_some_and(|bytes| bytes >= self.0)
    }
}

#[tokio::test]
async fn download_is_time_bounded_and_reports_measured_bytes() {
    let server = MockServer::start().await;
    // The real `__down` contract (F4): serves exactly what is asked, refuses
    // the 403 ceiling. This request never gets near it, but the mock still
    // enforces the same ceiling every other `__down` test does.
    Mock::given(method("GET"))
        .and(path("/__down"))
        .respond_with(LikeCloudflareDown)
        .mount(&server)
        .await;
    let p = CloudflareProvider::new(EndpointConfig {
        download_url: format!("{}/__down", server.uri()),
        upload_url: format!("{}/__up", server.uri()),
    });
    let t = p.download(Duration::from_secs(2), Some(2_000_000)).await.unwrap();
    assert!(t.bytes > 0 && t.bytes <= 2_000_000, "bytes {}", t.bytes);
    assert!(t.duration <= Duration::from_secs(3), "duration {:?}", t.duration);
    assert!(t.bits_per_sec > 0.0);
}

#[tokio::test]
async fn upload_posts_a_body_and_measures_it() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/__up"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let p = CloudflareProvider::new(EndpointConfig {
        download_url: format!("{}/__down", server.uri()),
        upload_url: format!("{}/__up", server.uri()),
    });
    let t = p.upload(Duration::from_secs(2), Some(1_000_000)).await.unwrap();
    assert!(t.bytes > 0 && t.bytes <= 1_000_000);
    assert!(t.bits_per_sec > 0.0);
}

#[tokio::test]
async fn a_server_error_is_an_error_not_a_zero_reading() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/__down"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let p = CloudflareProvider::new(EndpointConfig {
        download_url: format!("{}/__down", server.uri()),
        upload_url: format!("{}/__up", server.uri()),
    });
    assert!(p.download(Duration::from_secs(1), Some(1_000_000)).await.is_err());
}

/// Proves the deadline — not the byte cap — is what stops a slow download.
/// The mock body is far larger than a 20 ms budget can drain over any
/// realistic link (including loopback), so `download()` must return early
/// with `bytes < max_bytes`. If the budget logic were ever deleted, this
/// body would still get read to completion and `bytes == max_bytes`, and
/// the whole call would very likely finish well under `budget`, failing
/// the `elapsed >= budget` assertion too — two independent tripwires.
#[tokio::test]
async fn download_stops_on_the_clock_not_the_byte_cap() {
    let server = MockServer::start().await;
    let max_bytes: u64 = 64_000_000; // 64 MB — no budget this small can drain it
    // F4: `LikeCloudflareDown` serves exactly the requested `bytes` (up to
    // 64 MB here) and enforces the real 403 ceiling, same as every other
    // `__down` mock in this file.
    Mock::given(method("GET"))
        .and(path("/__down"))
        .respond_with(LikeCloudflareDown)
        .mount(&server)
        .await;
    let p = CloudflareProvider::new(EndpointConfig {
        download_url: format!("{}/__down", server.uri()),
        upload_url: format!("{}/__up", server.uri()),
    });
    let budget = Duration::from_millis(20);
    let slack = Duration::from_secs(2);

    let call_start = Instant::now();
    let t = p.download(budget, Some(max_bytes)).await.unwrap();
    let elapsed = call_start.elapsed();

    assert!(elapsed >= budget, "elapsed {:?} < budget {:?}", elapsed, budget);
    assert!(
        elapsed < budget + slack,
        "elapsed {:?} >= budget+slack {:?}",
        elapsed,
        budget + slack
    );
    assert!(
        t.bytes < max_bytes,
        "bytes {} not < max_bytes {} — the clock did not stop the read",
        t.bytes,
        max_bytes
    );
}

/// Same property for upload: a body far larger than a 20 ms budget can push
/// (even over loopback), so the `UploadStream` must stop yielding chunks on
/// `deadline`, not on `remaining == 0`.
#[tokio::test]
async fn upload_stops_on_the_clock_not_the_byte_cap() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/__up"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let p = CloudflareProvider::new(EndpointConfig {
        download_url: format!("{}/__down", server.uri()),
        upload_url: format!("{}/__up", server.uri()),
    });
    let max_bytes: u64 = 64_000_000; // 64 MB — no budget this small can push it
    let budget = Duration::from_millis(20);
    let slack = Duration::from_secs(2);

    let call_start = Instant::now();
    let t = p.upload(budget, Some(max_bytes)).await.unwrap();
    let elapsed = call_start.elapsed();

    assert!(elapsed >= budget, "elapsed {:?} < budget {:?}", elapsed, budget);
    assert!(
        elapsed < budget + slack,
        "elapsed {:?} >= budget+slack {:?}",
        elapsed,
        budget + slack
    );
    assert!(
        t.bytes < max_bytes,
        "bytes {} not < max_bytes {} — the clock did not stop the send",
        t.bytes,
        max_bytes
    );
}

/// C1 — the live defect. The first acceptance run printed
/// `download: skipped` / `skip reason: provider: server returned status 403`
/// because the CLI's phase ceiling (100 MiB = 104,857,600) was passed
/// straight through as one request's `bytes` value, and `__down` refuses
/// anything from 100,000,000 up.
///
/// The mock refuses exactly what the real service refuses, so a phase
/// ceiling above that limit must now still produce a measurement — and no
/// individual request may reach the ceiling.
#[tokio::test]
async fn no_download_request_ever_reaches_the_403_ceiling() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/__down"))
        .and(BytesAtLeast(MAX_REQUEST_BYTES))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/__down"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![7u8; 64 * 1024]))
        .mount(&server)
        .await;

    let provider = CloudflareProvider::new(endpoints(&server));
    let measured = provider
        .download(Duration::from_millis(200), Some(100 * 1024 * 1024))
        .await
        .expect("a phase ceiling above the per-request limit must still measure, not 403");

    assert!(measured.bytes > 0, "nothing was measured");
    let asked: Vec<u64> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter_map(requested_bytes)
        .collect();
    assert!(!asked.is_empty(), "no download request was issued at all");
    assert!(
        asked.iter().all(|&bytes| bytes < MAX_REQUEST_BYTES),
        "a request asked for {:?} bytes, at or above the 403 ceiling {MAX_REQUEST_BYTES}",
        asked.iter().max()
    );
}

/// C2 — the phase must be bounded by the clock, not by what one request can
/// deliver. The server caps a single request below 100 MB, so a
/// single-request phase measures one chunk and then stops, however much
/// budget is left: on a fast link that is a fraction of a second of a ten
/// second window, and the rest of the round's "ping under load" then runs
/// under no load at all.
///
/// The request count is what discriminates here. A rate assertion on its own
/// cannot: an implementation that stops early also reports a short
/// `duration`, so its bytes/duration still looks fast (see the task-7a
/// report). Both are asserted; only the count fails under the revert.
#[tokio::test]
async fn download_is_not_capped_by_what_one_request_can_deliver() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/__down"))
        .respond_with(LikeCloudflareDown)
        .mount(&server)
        .await;

    // A chunk far below the real 25 MB so the budget spans many requests in
    // milliseconds; the property under test does not depend on the size.
    let chunk: u64 = 256 * 1024;
    let budget = Duration::from_millis(500);
    let provider = CloudflareProvider::with_download_chunk_bytes(endpoints(&server), chunk);

    let measured = provider.download(budget, Some(8_000_000)).await.unwrap();

    let asked: Vec<u64> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter_map(requested_bytes)
        .collect();
    assert!(
        asked.len() >= 2,
        "the phase issued {} request(s); a time-bounded download keeps asking until the budget ends",
        asked.len()
    );
    assert!(
        asked.iter().all(|&bytes| bytes <= chunk),
        "a request asked for more than one chunk: {asked:?}"
    );
    assert!(
        measured.bytes > chunk,
        "measured {} bytes, no more than a single request could deliver ({chunk})",
        measured.bytes
    );
    let one_request_ceiling = chunk as f64 * 8.0 / budget.as_secs_f64();
    assert!(
        measured.bits_per_sec > one_request_ceiling,
        "reported {:.0} bit/s, capped by one request's worth over the budget ({one_request_ceiling:.0} bit/s)",
        measured.bits_per_sec
    );
}

/// The documented failure policy: bytes that already crossed the wire are a
/// measurement. Losing the first eight seconds of a phase because the ninth
/// second broke would discard the better of the two answers.
#[tokio::test]
async fn a_chunk_failing_after_bytes_were_measured_keeps_the_measurement() {
    let server = MockServer::start().await;
    let chunk: u64 = 256 * 1024;
    Mock::given(method("GET"))
        .and(path("/__down"))
        .respond_with(LikeCloudflareDown)
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/__down"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let provider = CloudflareProvider::with_download_chunk_bytes(endpoints(&server), chunk);
    let measured = provider
        .download(Duration::from_millis(300), Some(8_000_000))
        .await
        .expect("bytes already measured must survive a later chunk failing");

    assert_eq!(measured.bytes, chunk);
    assert!(measured.bits_per_sec > 0.0);
}

/// The other half of C2: `upload` needs no chunked-request treatment. Its
/// chunks are frames of ONE request's streaming body, not separate requests,
/// and `__up` has no per-request ceiling to run into — so a chunk size can
/// never cap the phase.
#[tokio::test]
async fn upload_is_not_capped_by_its_chunk_size() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/__up"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let provider = CloudflareProvider::new(endpoints(&server));
    let budget = Duration::from_millis(300);
    let measured = provider.upload(budget, Some(64_000_000)).await.unwrap();

    let one_chunk_ceiling = UPLOAD_CHUNK_BYTES as f64 * 8.0 / budget.as_secs_f64();
    assert!(
        measured.bytes > UPLOAD_CHUNK_BYTES as u64,
        "measured {} bytes, no more than one chunk ({UPLOAD_CHUNK_BYTES})",
        measured.bytes
    );
    assert!(
        measured.bits_per_sec > one_chunk_ceiling,
        "reported {:.0} bit/s, capped by one chunk over the budget ({one_chunk_ceiling:.0} bit/s)",
        measured.bits_per_sec
    );
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        1,
        "the whole upload phase is one request; its chunks are body frames"
    );
}

/// **F3.** No ceiling is the default (updated D6), and then the clock is the
/// only thing that may end the phase. The server here will serve any chunk
/// asked of it, forever, so a phase that stops early stopped for a reason
/// that is not allowed to exist.
#[tokio::test]
async fn a_download_with_no_ceiling_runs_the_whole_budget_and_is_not_capped() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/__down"))
        .respond_with(LikeCloudflareDown)
        .mount(&server)
        .await;

    let chunk: u64 = 256 * 1024;
    let budget = Duration::from_millis(300);
    let provider = CloudflareProvider::with_download_chunk_bytes(endpoints(&server), chunk);

    let call_start = Instant::now();
    let measured = provider.download(budget, None).await.unwrap();
    let elapsed = call_start.elapsed();

    assert!(
        !measured.capped,
        "nothing could have capped a phase with no ceiling"
    );
    assert!(
        elapsed >= budget,
        "the phase returned after {elapsed:?}, before its {budget:?} budget was spent"
    );
    assert!(measured.bytes > chunk, "measured {} bytes", measured.bytes);
}

/// **F3.** A ceiling still bounds the phase when one is given — that is how
/// a nearly-spent daily data budget stops the app overrunning it — and the
/// phase reports that the ceiling, not the clock, is what ended it.
#[tokio::test]
async fn a_download_stopped_by_its_ceiling_reports_itself_capped() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/__down"))
        .respond_with(LikeCloudflareDown)
        .mount(&server)
        .await;

    let chunk: u64 = 64 * 1024;
    let ceiling = chunk * 2;
    // Generous budget: only the ceiling can end this phase.
    let provider = CloudflareProvider::with_download_chunk_bytes(endpoints(&server), chunk);

    let measured = provider
        .download(Duration::from_secs(10), Some(ceiling))
        .await
        .unwrap();

    assert_eq!(measured.bytes, ceiling, "the ceiling must bound the phase");
    assert!(
        measured.capped,
        "a phase the ceiling ended must say so, or a truncated measurement reads as a normal one"
    );
}

async fn received_get_count(server: &MockServer) -> usize {
    server.received_requests().await.unwrap().len()
}

/// F2. A 429 on the first chunk is not a one-shot skip that the next
/// `download()` immediately retries. The second call must return the rate-
/// limit condition without issuing another request.
#[tokio::test]
async fn a_429_is_not_retried_before_backoff_elapses() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/__down"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;

    let provider = CloudflareProvider::with_download_chunk_bytes(endpoints(&server), 64 * 1024);
    let budget = Duration::from_millis(200);

    let first = provider.download(budget, None).await.unwrap_err();
    match &first {
        EngineError::RateLimited(limit) => {
            assert_eq!(limit.consecutive, 1);
            assert_eq!(limit.retry_after, Duration::from_secs(15 * 60));
        }
        other => panic!("first 429 must be RateLimited, got {other}"),
    }
    assert!(
        first.to_string().contains("rate-limiting us"),
        "skip reason must be distinguishable from a generic status: {first}"
    );
    assert_eq!(received_get_count(&server).await, 1);

    let second = provider.download(budget, None).await.unwrap_err();
    match &second {
        EngineError::RateLimited(limit) => {
            assert_eq!(
                limit.consecutive, 1,
                "a skip inside the same backoff is the same condition, not a new 429"
            );
            assert!(limit.retry_after > Duration::ZERO);
            assert!(limit.retry_after <= Duration::from_secs(15 * 60));
        }
        other => panic!("backoff skip must be RateLimited, got {other}"),
    }
    assert_eq!(
        received_get_count(&server).await,
        1,
        "a 429 must not be retried before the backoff elapses"
    );
}

/// F2. When the server sends `Retry-After` as delay-seconds, that value is
/// the wait — not the built-in 15-minute schedule.
#[tokio::test]
async fn retry_after_delay_seconds_are_honoured() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/__down"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "1"))
        .mount(&server)
        .await;

    let provider = CloudflareProvider::with_download_chunk_bytes(endpoints(&server), 64 * 1024);
    let budget = Duration::from_millis(200);

    let first = provider.download(budget, None).await.unwrap_err();
    match &first {
        EngineError::RateLimited(limit) => {
            assert_eq!(limit.retry_after, Duration::from_secs(1));
            assert_eq!(limit.consecutive, 1);
        }
        other => panic!("expected RateLimited honouring Retry-After, got {other}"),
    }
    assert_eq!(received_get_count(&server).await, 1);

    tokio::time::sleep(Duration::from_millis(200)).await;
    let _still_waiting = provider.download(budget, None).await.unwrap_err();
    assert_eq!(
        received_get_count(&server).await,
        1,
        "must not retry before Retry-After elapses"
    );

    tokio::time::sleep(Duration::from_millis(900)).await;
    let after = provider.download(budget, None).await.unwrap_err();
    match &after {
        EngineError::RateLimited(limit) => {
            assert_eq!(
                limit.consecutive, 2,
                "a 429 after the wait is the same limit still active — the sustained case"
            );
        }
        other => panic!("expected a second 429 to stay RateLimited, got {other}"),
    }
    assert_eq!(
        received_get_count(&server).await,
        2,
        "Retry-After of 1s was not honoured — the client did not come back after the wait"
    );
    assert!(
        after.to_string().contains("sustained"),
        "two 429s must read as one sustained condition, not two hiccups: {after}"
    );
}

/// F2. Bytes that already crossed the wire survive a 429 on a later chunk.
/// The phase is a measurement, not a discarded attempt; backoff is still
/// armed so the next call does not hammer the limit.
#[tokio::test]
async fn a_429_after_chunks_were_measured_keeps_the_bytes() {
    let server = MockServer::start().await;
    let chunk: u64 = 256 * 1024;
    Mock::given(method("GET"))
        .and(path("/__down"))
        .respond_with(LikeCloudflareDown)
        .up_to_n_times(2)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/__down"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;

    let provider = CloudflareProvider::with_download_chunk_bytes(endpoints(&server), chunk);
    let measured = provider
        .download(Duration::from_millis(300), Some(8_000_000))
        .await
        .expect("bytes already measured must survive a later 429");

    assert_eq!(measured.bytes, chunk * 2);
    assert!(measured.bits_per_sec > 0.0);
    assert_eq!(received_get_count(&server).await, 3, "two successes then one 429");

    let skipped = provider
        .download(Duration::from_millis(300), Some(8_000_000))
        .await
        .unwrap_err();
    assert!(
        matches!(skipped, EngineError::RateLimited(_)),
        "the 429 must arm backoff even though this phase kept its bytes: {skipped}"
    );
    assert_eq!(
        received_get_count(&server).await,
        3,
        "the follow-up call must not hit the server while backoff is in force"
    );
}
