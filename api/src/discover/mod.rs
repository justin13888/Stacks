pub mod dto;
pub mod error;
pub mod handlers;

use salvo::prelude::*;

pub fn router() -> Router {
    Router::with_path("discover")
        .push(Router::with_path("trending").get(handlers::trending))
        .push(Router::with_path("popular").get(handlers::popular))
        .push(Router::with_path("top-rated").get(handlers::top_rated))
}
