pub mod dto;
pub mod error;
pub mod handlers;

use salvo::prelude::*;

pub fn router() -> Router {
    Router::new()
        .push(Router::with_path("media/{cameo_id}/availability").get(handlers::get_availability))
        .push(Router::with_path("servers").get(handlers::list_servers))
        .push(Router::with_path("servers/{id}/status").get(handlers::get_server_status))
}
