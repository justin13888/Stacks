pub mod dto;
pub mod error;
pub mod handlers;

use salvo::prelude::*;

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
