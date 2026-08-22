pub mod dto;
pub mod error;
pub mod handlers;

use salvo::prelude::*;

pub fn router() -> Router {
    Router::new()
        .push(
            Router::with_path("users/me/blocklist")
                .get(handlers::list_blocklist)
                .post(handlers::add_to_blocklist),
        )
        .push(
            Router::with_path("users/me/blocklist/{cameo_id}")
                .delete(handlers::remove_from_blocklist),
        )
}
