use salvo::oapi::OpenApi;
use salvo::prelude::*;
use sea_orm::DatabaseConnection;

use stacks_core::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: Config,
}

/// Build the full Salvo router. Used by both `stacks-api` and `gen-openapi`.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .hoop(
            salvo::cors::Cors::new()
                .allow_origin(salvo::cors::Any)
                .allow_methods(salvo::cors::Any)
                .allow_headers(salvo::cors::Any)
                .into_handler(),
        )
        .hoop(crate::middleware::logging::LoggingMiddleware)
        .hoop(crate::middleware::security_headers::SecurityHeaders)
        .hoop(salvo::affix_state::inject(state))
        .push(
            Router::with_path("api/v1")
                .push(crate::auth::router())
                .push(crate::user::router())
                .push(crate::media::router())
                .push(crate::discover::router())
                .push(crate::watch_state::router())
                .push(crate::watch_history::router())
                .push(crate::lists::router())
                .push(crate::tags::router())
                .push(crate::blocklist::router())
                .push(crate::streaming::router())
                .push(crate::notifications::router())
                .push(crate::import_export::router())
                .push(crate::admin::router())
                .push(crate::stats::router())
                .push(crate::webhooks::router()),
        )
        .push(crate::health::router())
}

/// Generate the OpenAPI document from the router (no live DB required).
pub fn openapi_doc() -> OpenApi {
    let router = build_router_stub();
    OpenApi::new("Stacks API", "0.1.0").merge_router(&router)
}

/// Stub router for spec generation — uses a dummy AppState.
fn build_router_stub() -> Router {
    // We only need the router shape for OpenAPI extraction, not a live DB.
    Router::new()
        .push(
            Router::with_path("api/v1")
                .push(crate::auth::router())
                .push(crate::user::router())
                .push(crate::media::router())
                .push(crate::discover::router())
                .push(crate::watch_state::router())
                .push(crate::watch_history::router())
                .push(crate::lists::router())
                .push(crate::tags::router())
                .push(crate::blocklist::router())
                .push(crate::streaming::router())
                .push(crate::notifications::router())
                .push(crate::import_export::router())
                .push(crate::admin::router())
                .push(crate::stats::router())
                .push(crate::webhooks::router()),
        )
        .push(crate::health::router())
}
