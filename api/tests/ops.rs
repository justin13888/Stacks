//! Probes and metrics.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use api::ops::{Liveness, Operator, Readiness};
use kynos::http::StatusCode;
use kynos::test::{Coverage, TestClient};
use sea_orm::ConnectionTrait;

#[tokio::test]
async fn probes_report_liveness_and_readiness() {
    let env = env!();
    let client = env.client();

    let live: Liveness = client
        .get("/healthz")
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(live.status, "ok");

    let ready: Readiness = client
        .get("/readyz")
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(ready.status, "ready");

    // A database that will not answer is not ready.
    let closed = env.closed_client().await;
    closed
        .get("/readyz")
        .send()
        .await
        .assert_status(StatusCode::SERVICE_UNAVAILABLE)
        .assert_problem_type(&support::problem("database-unavailable"));
    closed.assert_conformance();

    support::sweep::sweep(&env, "ops").await;

    // Nor is a schema behind this release.
    env.db
        .db
        .execute_unprepared("DELETE FROM seaql_migrations")
        .await
        .expect("forget the applied migrations");
    client
        .get("/readyz")
        .send()
        .await
        .assert_status(StatusCode::SERVICE_UNAVAILABLE)
        .assert_problem_type(&support::problem("migrations-pending"));
    client.assert_conformance();
    env.assert_covered("ops");
}

fn operator(token: Option<&str>, coverage: &Coverage) -> TestClient<Operator> {
    let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
    let operator = Operator {
        prometheus: recorder.handle(),
        token: token.map(str::to_owned),
    };
    TestClient::new(
        api::app::internal_router()
            .build(operator)
            .expect("the operator router builds"),
    )
    .with_coverage(coverage)
}

#[tokio::test]
async fn metrics_are_served_behind_the_configured_token() {
    let coverage = Coverage::new();
    let guarded = operator(Some("scrape-me"), &coverage);
    guarded
        .get("/metrics")
        .send()
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    guarded
        .get("/metrics")
        .header("authorization", "Bearer wrong")
        .send()
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    let scraped = guarded
        .get("/metrics")
        .header("authorization", "Bearer scrape-me")
        .send()
        .await;
    scraped.assert_status(StatusCode::OK);
    assert!(
        scraped
            .header("content-type")
            .is_some_and(|c| c.starts_with("text/plain"))
    );
    guarded.assert_conformance();
    // Every guard declares a 403, and this one has nothing to forbid: a
    // scraper's token is right (200) or wrong (401). The 403 is kynos's
    // declaration, not a response the service can send, so it is the one
    // declared response left out of the coverage check (getkono/kynos#1001
    // asks for a way to declare 401 alone).
    let mut document = api::app::internal_router()
        .openapi_as(kynos::openapi::SpecVersion::V3_1)
        .expect("3.1 emission");
    let metrics = document
        .paths
        .items
        .get_mut("/metrics")
        .and_then(|item| item.get.as_mut())
        .expect("GET /metrics");
    metrics.responses.responses.shift_remove("403");
    coverage.assert_declared_responses_covered(&document);

    // With no token configured, anyone who reaches the listener may scrape.
    let open = operator(None, &coverage);
    open.get("/metrics")
        .send()
        .await
        .assert_status(StatusCode::OK);
    open.assert_conformance();
}

#[test]
fn the_operator_document_validates_and_is_3_1() {
    let router = api::app::internal_router();
    let violations = router.validate().expect("validation runs");
    assert!(violations.is_empty(), "violations: {violations:?}");
    let document = router
        .openapi_as(kynos::openapi::SpecVersion::V3_1)
        .expect("3.1 emission");
    assert!(document.openapi.starts_with("3.1."));
}
