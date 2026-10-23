pub mod dto;
pub mod error;
pub mod handlers;

use salvo::prelude::*;

pub fn router() -> Router {
    Router::with_path("admin")
        .push(Router::with_path("jobs").get(handlers::list_jobs))
        .push(Router::with_path("jobs/{id}/retry").post(handlers::retry_job))
}
