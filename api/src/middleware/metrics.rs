//! Prometheus metrics collection middleware.
//! TODO: integrate with prometheus crate and expose /metrics endpoint.

use salvo::prelude::*;

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
        ctrl.call_next(req, depot, res).await;
        // TODO: record request count, duration histogram, status code labels
    }
}
