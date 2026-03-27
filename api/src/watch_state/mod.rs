pub mod dto;
pub mod error;
pub mod handlers;

use salvo::prelude::*;

pub fn router() -> Router {
    Router::new()
        .push(
            Router::with_path("users/me/media/<cameo_id>/status")
                .get(handlers::get_media_status)
                .put(handlers::set_media_status)
                .delete(handlers::delete_media_status),
        )
        .push(
            Router::with_path("users/me/episodes/<episode_id>/status")
                .get(handlers::get_episode_status)
                .put(handlers::set_episode_status),
        )
        .push(Router::with_path("users/me/media/bulk-status").post(handlers::bulk_update_status))
}
