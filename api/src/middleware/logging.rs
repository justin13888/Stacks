use salvo::prelude::*;
use tracing::info;

pub struct LoggingMiddleware;

#[async_trait]
impl Handler for LoggingMiddleware {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let method = req.method().clone();
        let uri = req.uri().path().to_string();
        ctrl.call_next(req, depot, res).await;
        let status = res.status_code.unwrap_or(StatusCode::OK);
        info!(method = %method, path = %uri, status = %status, "request");
    }
}
