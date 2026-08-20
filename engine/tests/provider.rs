use alidade_engine::{CloudflareProvider, EndpointConfig, SpeedProvider};
use std::time::Duration;
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
