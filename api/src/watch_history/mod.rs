pub mod dto;
pub mod error;
pub mod handlers;

use salvo::prelude::*;

pub fn router() -> Router {
    Router::with_path("users/me/history")
        .get(handlers::list_history)
        .post(handlers::add_history)
}
