use alidade_engine::{CloudflareProvider, EndpointConfig, SpeedProvider};
use std::time::{Duration, Instant};
use wiremock::{
    matchers::{method, path, query_param},
    Mock, MockServer, ResponseTemplate,
};

#[tokio::test]
async fn download_is_time_bounded_and_reports_measured_bytes() {
    let server = MockServer::start().await;
    // 2 MB body, served instantly; the provider must stop at the budget or the byte cap.
    Mock::given(method("GET"))
        .and(path("/__down"))
        .and(query_param("bytes", "2000000"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![7u8; 2_000_000]))
        .mount(&server)
        .await;
    let p = CloudflareProvider::new(EndpointConfig {
        download_url: format!("{}/__down", server.uri()),
        upload_url: format!("{}/__up", server.uri()),
    });
    let t = p.download(Duration::from_secs(2), 2_000_000).await.unwrap();
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
    let t = p.upload(Duration::from_secs(2), 1_000_000).await.unwrap();
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
    assert!(p.download(Duration::from_secs(1), 1_000_000).await.is_err());
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
    Mock::given(method("GET"))
        .and(path("/__down"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![7u8; max_bytes as usize]))
        .mount(&server)
        .await;
    let p = CloudflareProvider::new(EndpointConfig {
        download_url: format!("{}/__down", server.uri()),
        upload_url: format!("{}/__up", server.uri()),
    });
    let budget = Duration::from_millis(20);
    let slack = Duration::from_secs(2);

    let call_start = Instant::now();
    let t = p.download(budget, max_bytes).await.unwrap();
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
    let t = p.upload(budget, max_bytes).await.unwrap();
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
