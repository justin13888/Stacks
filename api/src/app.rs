use std::sync::Arc;
use std::time::Duration;

use metrics_exporter_prometheus::PrometheusHandle;
use salvo::oapi::OpenApi;
use salvo::prelude::*;
use sea_orm::DatabaseConnection;

use stacks_core::config::Config;
use stacks_core::media::MediaClient;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: Config,
    /// Shared cameo media-metadata client.
    pub cameo: Arc<MediaClient>,
    /// Prometheus handle — `Some` at runtime, `None` for the spec generator
    /// (which never serves requests, so never renders).
    pub metrics: Option<Arc<PrometheusHandle>>,
}

/// Build the full Salvo router. Used by both `stacks-api` and `gen-openapi`.
pub fn build_router(state: AppState) -> Router {
    let cors = crate::middleware::cors::build(&state.config.server.cors_allowed_origins);

    let router = Router::new()
        .hoop(cors)
        .hoop(crate::middleware::metrics::MetricsMiddleware)
        .hoop(crate::middleware::logging::LoggingMiddleware)
        .hoop(crate::middleware::security_headers::SecurityHeaders)
        .hoop(salvo::affix_state::inject(state))
        .push(
            Router::with_path("api/v1")
                // Public routes — no auth required, but rate-limited to slow
                // brute-force credential probing.
                .push(
                    Router::new()
                        .hoop(crate::middleware::rate_limit::RateLimiter::new(
                            30,
                            Duration::from_secs(60),
                        ))
                        .push(crate::auth::public_router()),
                )
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
        .push(crate::health::router())
        // Public Prometheus scrape endpoint. Restrict at the proxy layer when
        // exposed beyond a trusted network.
        .push(Router::with_path("metrics").get(crate::middleware::metrics::metrics_handler));

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
        .push(Router::with_path("metrics").get(crate::middleware::metrics::metrics_handler))
}
