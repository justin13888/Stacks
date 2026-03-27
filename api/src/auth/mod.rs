pub mod dto;
pub mod error;
pub mod handlers;

use salvo::prelude::*;

pub fn router() -> Router {
    Router::with_path("auth")
        .push(Router::with_path("login").post(handlers::oidc_login))
        .push(Router::with_path("callback").get(handlers::oidc_callback))
        .push(Router::with_path("refresh").post(handlers::refresh_token))
        .push(Router::with_path("logout").post(handlers::logout))
        .push(
            Router::with_path("tokens")
                .get(handlers::list_tokens)
                .post(handlers::create_token),
        )
        .push(Router::with_path("tokens/<id>").delete(handlers::delete_token))
}
