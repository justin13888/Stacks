//! Request metrics, labelled by operation (docs/adr/0005-observability.md).
//!
//! A kynos [`Observer`] sees every request and the operation it matched, so
//! the labels are the description's `operationId`s rather than raw paths: a
//! client cannot mint a new time series by inventing URLs, and a dashboard
//! reads in the API's own vocabulary.

use std::time::Duration;

use kynos::http::{Request, Response};
use kynos::middleware::Observer;
use kynos::router::operation::Route;

/// The label a request that matched no operation is counted under.
const UNMATCHED: &str = "unmatched";

/// Counts and times every response.
///
/// - `stacks_http_requests_total{operation, status}`
/// - `stacks_http_request_duration_seconds{operation, status}`, the time to
///   produce the response head.
pub struct Metrics;

impl<C> Observer<C> for Metrics {
    fn on_request(&self, _request: &Request, _route: Option<Route<'_>>, _context: &C) {}

    fn on_response(&self, response: &Response, route: Option<Route<'_>>, elapsed: Duration) {
        let operation = route.map_or(UNMATCHED, |r| r.operation_id()).to_owned();
        let status = response.status().as_u16().to_string();
        metrics::counter!(
            "stacks_http_requests_total",
            "operation" => operation.clone(),
            "status" => status.clone(),
        )
        .increment(1);
        metrics::histogram!(
            "stacks_http_request_duration_seconds",
            "operation" => operation,
            "status" => status,
        )
        .record(elapsed.as_secs_f64());
    }
}
