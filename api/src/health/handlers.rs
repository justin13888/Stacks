use salvo::prelude::*;
use sea_orm::{ConnectionTrait, DbBackend, Statement};

use crate::app::AppState;

/// Returns 200 OK with `{"status":"ok"}` — always succeeds.
#[salvo::oapi::endpoint(tags("Health"), summary = "Liveness check")]
pub async fn health_check(res: &mut Response) {
    res.status_code(StatusCode::OK);
    res.render(salvo::writing::Json(serde_json::json!({ "status": "ok" })));
}

/// Checks DB connectivity; returns 200 or 503.
#[salvo::oapi::endpoint(tags("Health"), summary = "Readiness check")]
pub async fn health_ready(depot: &mut Depot, res: &mut Response) {
    let state = match depot.obtain::<AppState>() {
        Ok(s) => s.clone(),
        Err(_) => {
            res.status_code(StatusCode::SERVICE_UNAVAILABLE);
            res.render(salvo::writing::Json(serde_json::json!({
                "status": "unavailable",
                "reason": "app state not initialised"
            })));
            return;
        }
    };

    match state
        .db
        .execute(Statement::from_string(DbBackend::Postgres, "SELECT 1"))
        .await
    {
        Ok(_) => {
            res.status_code(StatusCode::OK);
            res.render(salvo::writing::Json(serde_json::json!({ "status": "ok" })));
        }
        Err(e) => {
            res.status_code(StatusCode::SERVICE_UNAVAILABLE);
            res.render(salvo::writing::Json(serde_json::json!({
                "status": "unavailable",
                "reason": e.to_string()
            })));
        }
    }
}
