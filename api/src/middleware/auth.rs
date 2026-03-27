//! JWT / API token extraction middleware.
//! Injects the authenticated `user::Model` into the Depot under the key "auth_user".
//! Routes that don't need auth skip this middleware — it is applied per-router.

use salvo::prelude::*;

/// Key used to store the authenticated user in the Depot.
pub const AUTH_USER_KEY: &str = "auth_user";

pub struct AuthMiddleware;

#[async_trait]
impl Handler for AuthMiddleware {
    async fn handle(
        &self,
        _req: &mut Request,
        _depot: &mut Depot,
        _res: &mut Response,
        _ctrl: &mut FlowCtrl,
    ) {
        todo!("JWT/API token extraction — validate Bearer token, inject user into depot")
    }
}
