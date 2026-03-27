//! Admin role guard. Rejects non-admin users with 403.

use salvo::prelude::*;

use super::auth::AUTH_USER_KEY;

pub struct AdminGuard;

#[async_trait]
impl Handler for AdminGuard {
    async fn handle(
        &self,
        _req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let user = depot
            .get::<stacks_core::entity::user::Model>(AUTH_USER_KEY)
            .ok();
        match user {
            Some(u) if u.role == "admin" => {
                ctrl.call_next(_req, depot, res).await;
            }
            _ => {
                res.status_code(StatusCode::FORBIDDEN);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "FORBIDDEN", "message": "Admin access required" }
                })));
            }
        }
    }
}
