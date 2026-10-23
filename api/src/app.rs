use std::sync::Arc;
use std::time::Duration;

use metrics_exporter_prometheus::PrometheusHandle;
use salvo::oapi::OpenApi;
use salvo::prelude::*;
use sea_orm::DatabaseConnection;

use stacks_core::config::Config;
use stacks_core::media::MetadataProvider;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: Config,
    /// The shared metadata provider (cameo behind Stacks' resilience layer).
    pub metadata: Arc<dyn MetadataProvider>,
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
                        // Media browsing, discovery, import/export, stats.
                        .push(crate::media::router())
                        .push(crate::discover::router())
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
    let router = router
        .push(Router::with_path("api-docs/openapi.json").get(openapi_spec_handler))
        .push(salvo::oapi::scalar::Scalar::new("/api-docs/openapi.json").into_router("/scalar"));

    router
}

/// Generate the OpenAPI document from the router (no live DB required).
pub fn openapi_doc() -> OpenApi {
    let router = build_router_stub();
    OpenApi::new("Stacks API", "0.1.0").merge_router(&router)
}

/// The OpenAPI document as JSON, with path parameters back-filled.
///
/// This is the single source of the spec: the `gen-openapi` binary, the
/// `/api-docs/openapi.json` route, and the Scalar UI all render it, so the spec
/// the server advertises is exactly the one the typed client is generated from.
/// The back-fill previously lived only in the binary, which left the running
/// server serving operations whose `{id}` placeholders were never declared.
///
/// Returns JSON rather than `OpenApi` because salvo's `OpenApi` does not
/// round-trip through serde — deserialising the post-processed value back into
/// the typed struct silently drops the injected parameters.
pub fn openapi_json() -> serde_json::Value {
    let mut json = serde_json::to_value(openapi_doc()).unwrap_or_else(|e| {
        tracing::error!(error = %e, "failed to serialize OpenAPI spec");
        serde_json::json!({})
    });
    inject_path_params(&mut json);
    json
}

/// Serve the post-processed OpenAPI document. Debug builds only.
#[cfg(debug_assertions)]
#[salvo::handler]
async fn openapi_spec_handler(res: &mut Response) {
    res.render(salvo::writing::Json(openapi_json()));
}

/// Ensure every `{name}` placeholder in a route template is declared as a path
/// parameter on each operation.
///
/// Handlers read path segments with `req.param::<T>("name")`, which
/// `#[salvo::oapi::endpoint]` cannot see, so it emits the operation without the
/// parameter — producing a spec that is technically invalid and that orval
/// refuses to generate from. Back-filling from the template keeps handlers terse
/// without a `ToParameters` struct per route.
fn inject_path_params(spec: &mut serde_json::Value) {
    use std::collections::HashSet;

    let Some(paths) = spec.get_mut("paths").and_then(|p| p.as_object_mut()) else {
        return;
    };
    for (path, item) in paths.iter_mut() {
        let placeholders: Vec<String> = path
            .split('{')
            .skip(1)
            .filter_map(|chunk| chunk.split('}').next().map(str::to_string))
            .filter(|n| !n.is_empty())
            .collect();
        if placeholders.is_empty() {
            continue;
        }
        let Some(item) = item.as_object_mut() else {
            continue;
        };
        for (method, op) in item.iter_mut() {
            if !matches!(
                method.as_str(),
                "get" | "post" | "put" | "delete" | "patch" | "head" | "options"
            ) {
                continue;
            }
            let Some(op) = op.as_object_mut() else {
                continue;
            };
            let params = op
                .entry("parameters".to_string())
                .or_insert_with(|| serde_json::json!([]));
            let Some(arr) = params.as_array_mut() else {
                continue;
            };
            let existing: HashSet<String> = arr
                .iter()
                .filter_map(|p| p.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect();
            for name in &placeholders {
                if existing.contains(name) {
                    continue;
                }
                arr.push(serde_json::json!({
                    "name": name,
                    "in": "path",
                    "required": true,
                    "schema": { "type": "string" },
                }));
            }
        }
    }
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
                .push(crate::import_export::router())
                .push(crate::stats::router())
                .push(crate::admin::router()),
        )
        .push(crate::health::router())
        .push(Router::with_path("metrics").get(crate::middleware::metrics::metrics_handler))
}
