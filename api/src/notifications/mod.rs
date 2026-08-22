pub mod dto;
pub mod error;
pub mod handlers;

use salvo::prelude::*;

pub fn router() -> Router {
    Router::new()
        .push(
            Router::with_path("users/me/notification-channels")
                .get(handlers::list_channels)
                .post(handlers::create_channel),
        )
        .push(
            Router::with_path("users/me/notification-channels/{id}")
                .put(handlers::update_channel)
                .delete(handlers::delete_channel),
        )
        .push(
            Router::with_path("users/me/notification-preferences")
                .get(handlers::get_preferences)
                .put(handlers::update_preferences),
        )
}
