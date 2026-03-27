pub mod handlers;

use salvo::prelude::*;

pub fn router() -> Router {
    Router::new()
        .push(Router::with_path("health").get(handlers::health_check))
        .push(Router::with_path("health/ready").get(handlers::health_ready))
}
