//! Request metrics are labelled by operation, never by path.
//!
//! Its own test binary, because the recorder is process-global.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use kynos::http::StatusCode;

#[tokio::test]
async fn requests_are_counted_under_their_operation_id() {
    let prometheus = metrics_exporter_prometheus::PrometheusBuilder::new()
        .install_recorder()
        .expect("the only recorder in this binary");
    let env = env!();
    let client = env.client();

    client
        .get("/healthz")
        .send()
        .await
        .assert_status(StatusCode::OK);
    client
        .get("/api/v1/no-such-thing/12345")
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND);

    let scraped = prometheus.render();
    let series = |name: &str, operation: &str, status: &str| {
        scraped.lines().any(|line| {
            line.starts_with(name)
                && line.contains(&format!("operation=\"{operation}\""))
                && line.contains(&format!("status=\"{status}\""))
        })
    };
    assert!(
        series("stacks_http_requests_total", "getLiveness", "200"),
        "{scraped}"
    );
    assert!(
        series("stacks_http_request_duration_seconds", "getLiveness", "200"),
        "{scraped}"
    );
    // A URL no operation matched cannot mint a series of its own.
    assert!(
        series("stacks_http_requests_total", "unmatched", "404"),
        "{scraped}"
    );
    assert!(!scraped.contains("no-such-thing"), "{scraped}");
}
