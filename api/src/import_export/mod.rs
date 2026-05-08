pub mod dto;
pub mod error;
pub mod handlers;

use salvo::prelude::*;

pub fn router() -> Router {
    Router::new()
        .push(Router::with_path("users/me/import").post(handlers::start_import))
        .push(Router::with_path("users/me/import/<job_id>").get(handlers::get_import_status))
        .push(Router::with_path("users/me/export").get(handlers::export_data))
}
