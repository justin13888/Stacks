use salvo::prelude::*;

/// Returns 200 OK with `{"status":"ok"}` — always succeeds.
#[salvo::oapi::endpoint(tags("Health"), summary = "Liveness check")]
pub async fn health_check(res: &mut Response) {
    res.status_code(StatusCode::OK);
    res.render(salvo::writing::Json(serde_json::json!({ "status": "ok" })));
}

/// Checks DB connectivity and dependent services; returns 200 or 503.
#[salvo::oapi::endpoint(tags("Health"), summary = "Readiness check")]
pub async fn health_ready(_req: &mut Request, _res: &mut Response) {
    todo!()
}
