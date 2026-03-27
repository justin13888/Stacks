//! Extractor: pull the authenticated user out of the Depot.

use salvo::prelude::*;

use crate::middleware::auth::AUTH_USER_KEY;

/// Extract the authenticated user from the Depot, returning 401 if not present.
pub fn require_user(depot: &Depot) -> Option<&stacks_core::entity::user::Model> {
    depot
        .get::<stacks_core::entity::user::Model>(AUTH_USER_KEY)
        .ok()
}

/// Return the user or render 401 and return None.
pub async fn extract_user<'a>(
    depot: &'a Depot,
    res: &mut Response,
) -> Option<&'a stacks_core::entity::user::Model> {
    match require_user(depot) {
        Some(u) => Some(u),
        None => {
            res.status_code(StatusCode::UNAUTHORIZED);
            res.render(salvo::writing::Json(serde_json::json!({
                "error": { "code": "UNAUTHORIZED", "message": "Authentication required" }
            })));
            None
        }
    }
}
