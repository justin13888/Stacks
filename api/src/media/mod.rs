pub mod dto;
pub mod error;
pub mod handlers;

use salvo::prelude::*;

pub fn router() -> Router {
    Router::new()
        .push(Router::with_path("media/search").get(handlers::search_media))
        .push(Router::with_path("media/<cameo_id>").get(handlers::get_media))
        .push(Router::with_path("media/<cameo_id>/seasons").get(handlers::get_seasons))
        .push(
            Router::with_path("media/<cameo_id>/seasons/<season>/episodes")
                .get(handlers::get_episodes),
        )
        .push(
            Router::with_path("media/<cameo_id>/recommendations")
                .get(handlers::get_recommendations),
        )
}
