pub mod dto;
pub mod error;
pub mod handlers;

use salvo::prelude::*;

/// User self-service routes (authenticated, any role).
pub fn user_router() -> Router {
    Router::new()
        .push(
            Router::with_path("users/me")
                .get(handlers::get_me)
                .patch(handlers::update_me),
        )
        .push(Router::with_path("users/me/settings").patch(handlers::update_settings))
}

/// Admin-only user management routes.
pub fn admin_router() -> Router {
    Router::new()
        .push(Router::with_path("users").get(handlers::list_users))
        .push(Router::with_path("users/<id>").delete(handlers::delete_user))
        .push(Router::with_path("users/<id>/role").patch(handlers::update_user_role))
}

/// Combined router — kept for backwards-compatibility.
#[allow(dead_code)]
pub fn router() -> Router {
    Router::new()
        .push(
            Router::with_path("users/me")
                .get(handlers::get_me)
                .patch(handlers::update_me),
        )
        .push(Router::with_path("users/me/settings").patch(handlers::update_settings))
        .push(Router::with_path("users").get(handlers::list_users))
        .push(Router::with_path("users/<id>").delete(handlers::delete_user))
}
