//! JWT / API token extraction middleware.
//! Injects the authenticated `user::Model` into the Depot under the key "auth_user".
//! Routes that don't need auth skip this middleware — it is applied per-router.

use salvo::prelude::*;
use stacks_core::auth::service::AuthService;

use crate::app::AppState;

/// Key used to store the authenticated user in the Depot.
pub const AUTH_USER_KEY: &str = "auth_user";

pub struct AuthMiddleware;

#[async_trait]
impl Handler for AuthMiddleware {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let token = match req.header::<String>("Authorization") {
            Some(h) if h.starts_with("Bearer ") => h[7..].to_string(),
            _ => {
                res.status_code(StatusCode::UNAUTHORIZED);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "UNAUTHORIZED", "message": "Authentication required" }
                })));
                return;
            }
        };

        let state = match depot.obtain::<AppState>() {
            Ok(s) => s.clone(),
            Err(_) => {
                res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "INTERNAL_ERROR", "message": "An internal error occurred" }
                })));
                return;
            }
        };

        let auth_svc = AuthService::new(
            state.db.clone(),
            state.config.auth.clone(),
            state.config.oidc.clone(),
        );

        // API tokens carry a recognisable prefix; anything else is a session
        // JWT. Either way a deactivated account is refused.
        let authenticated = if token.starts_with("stk_") {
            auth_svc.validate_api_token(&token, None).await
        } else {
            auth_svc.user_for_access_token(&token).await
        };
        let user = match authenticated {
            Ok(u) => u,
            Err(_) => {
                res.status_code(StatusCode::UNAUTHORIZED);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "UNAUTHORIZED", "message": "Authentication required" }
                })));
                return;
            }
        };

        depot.insert(AUTH_USER_KEY, user);
        ctrl.call_next(req, depot, res).await;
    }
}
