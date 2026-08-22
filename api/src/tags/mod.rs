pub mod dto;
pub mod error;
pub mod handlers;

use salvo::prelude::*;

pub fn router() -> Router {
    Router::new()
        .push(
            Router::with_path("users/me/tags")
                .get(handlers::list_tags)
                .post(handlers::create_tag),
        )
        .push(Router::with_path("users/me/tags/{id}").delete(handlers::delete_tag))
        .push(Router::with_path("users/me/media/{cameo_id}/tags").put(handlers::set_media_tags))
}
