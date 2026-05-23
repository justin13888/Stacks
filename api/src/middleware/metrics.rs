//! Prometheus request metrics + the public `/metrics` scrape endpoint.

use std::time::Instant;

use salvo::prelude::*;

use crate::app::AppState;

/// Salvo middleware that records request counts and durations.
///
/// Metric labels include `method`, `path`, and `status`. Note that `path` is
/// the raw request path, so cardinality grows with id-bearing routes — this
/// is acceptable for a single-tenant self-hosted deployment and would need
/// route templating before exposing to a fleet.
pub struct MetricsMiddleware;

#[async_trait]
impl Handler for MetricsMiddleware {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let started = Instant::now();
        let method = req.method().as_str().to_string();
        let path = req.uri().path().to_string();

        ctrl.call_next(req, depot, res).await;

        let status = res.status_code.map(|s| s.as_u16()).unwrap_or(200);
        let elapsed = started.elapsed().as_secs_f64();

        metrics::counter!(
            "http_requests_total",
            "method" => method.clone(),
            "path" => path.clone(),
            "status" => status.to_string(),
        )
        .increment(1);
        metrics::histogram!(
            "http_request_duration_seconds",
            "method" => method,
            "path" => path,
        )
        .record(elapsed);
    }
}

/// Public `/metrics` endpoint — renders the Prometheus text format from the
/// global recorder installed in `main`.
///
/// The endpoint is unauthenticated; restrict access at the reverse-proxy layer
/// when exposing it beyond the host network.
#[salvo::oapi::endpoint(tags("Observability"), summary = "Prometheus metrics")]
pub async fn metrics_handler(depot: &mut Depot) -> String {
    depot
        .obtain::<AppState>()
        .ok()
        .and_then(|s| s.metrics.as_ref().map(|h| h.render()))
        .unwrap_or_default()
}
