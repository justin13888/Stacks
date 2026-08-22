pub mod error;
pub mod handlers;

use salvo::prelude::*;

pub fn router() -> Router {
    Router::with_path("webhooks/beam/{server_id}").post(handlers::handle_beam_webhook)
}
