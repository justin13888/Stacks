use std::sync::Arc;

use salvo::oapi::OpenApi;
use salvo::prelude::*;
use sea_orm::DatabaseConnection;

use stacks_core::cameo::CameoClient;
use stacks_core::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: Config,
    /// Shared cameo media-metadata client.
    pub cameo: Arc<CameoClient>,
}

/// Build the full Salvo router. Used by both `stacks-api` and `gen-openapi`.
pub fn build_router(state: AppState) -> Router {
    let router = Router::new()
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
                // Public routes — no auth required.
                .push(crate::auth::public_router())
                // Beam webhooks authenticate via per-server HMAC signature,
                // not a user session, so the route sits outside the auth hoop.
                .push(crate::webhooks::router())
                // Authenticated routes — require valid JWT or API token.
                .push(
                    Router::new()
                        .hoop(crate::middleware::auth::AuthMiddleware)
                        .push(crate::auth::protected_router())
                        .push(crate::user::user_router())
                        .push(crate::watch_state::router())
                        .push(crate::watch_history::router())
                        .push(crate::lists::router())
                        .push(crate::tags::router())
                        .push(crate::blocklist::router())
                        .push(crate::notifications::router())
                        // Media browsing, discovery, streaming, import/export, stats.
                        .push(crate::media::router())
                        .push(crate::discover::router())
                        .push(crate::streaming::router())
                        .push(crate::import_export::router())
                        .push(crate::stats::router()),
                )
                // Admin-only routes — require auth + admin role.
                .push(
                    Router::new()
                        .hoop(crate::middleware::auth::AuthMiddleware)
                        .hoop(crate::middleware::admin::AdminGuard)
                        .push(crate::user::admin_router())
                        .push(crate::admin::router()),
                ),
        )
        .push(crate::health::router());

    #[cfg(debug_assertions)]
    let router = {
        let doc = openapi_doc();
        router
            .push(doc.into_router("/api-docs/openapi.json"))
            .push(salvo::oapi::scalar::Scalar::new("/api-docs/openapi.json").into_router("/scalar"))
    };

    router
}

/// Generate the OpenAPI document from the router (no live DB required).
pub fn openapi_doc() -> OpenApi {
    let router = build_router_stub();
    OpenApi::new("Stacks API", "0.1.0").merge_router(&router)
}

/// Stub router for spec generation — uses a dummy AppState.
fn build_router_stub() -> Router {
    Router::new()
        .push(
            Router::with_path("api/v1")
                .push(crate::auth::public_router())
                .push(crate::auth::protected_router())
                .push(crate::user::user_router())
                .push(crate::user::admin_router())
                .push(crate::watch_state::router())
                .push(crate::watch_history::router())
                .push(crate::lists::router())
                .push(crate::tags::router())
                .push(crate::blocklist::router())
                .push(crate::notifications::router())
                .push(crate::media::router())
                .push(crate::discover::router())
                .push(crate::streaming::router())
                .push(crate::import_export::router())
                .push(crate::stats::router())
                .push(crate::webhooks::router())
                .push(crate::admin::router()),
        )
        .push(crate::health::router())
}
