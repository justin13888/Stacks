//! CORS handler built from the configured origin allowlist.

use salvo::cors::{self, Cors, CorsHandler};

/// Origins assumed when nothing is configured — the Vite dev server and the
/// common React/Next dev ports.
const DEFAULT_LOCAL_ORIGINS: &[&str] = &[
    "http://localhost:5173",
    "http://localhost:3000",
    "http://127.0.0.1:5173",
    "http://127.0.0.1:3000",
];

/// Build a CORS handler from a configured origin allowlist.
///
/// - An empty list falls back to localhost developer defaults.
/// - A single `*` entry opens the API to any origin and logs a warning.
/// - Otherwise the listed origins are accepted verbatim.
pub fn build(allowed_origins: &[String]) -> CorsHandler {
    if allowed_origins.iter().any(|o| o == "*") {
        tracing::warn!(
            "WW_CORS_ALLOWED_ORIGINS includes '*' — CORS is wide open; do not use in production"
        );
        return Cors::new()
            .allow_origin(cors::Any)
            .allow_methods(cors::Any)
            .allow_headers(cors::Any)
            .into_handler();
    }

    let origin_strs: Vec<&str> = if allowed_origins.is_empty() {
        DEFAULT_LOCAL_ORIGINS.to_vec()
    } else {
        allowed_origins.iter().map(String::as_str).collect()
    };

    Cors::new()
        .allow_origin(origin_strs)
        .allow_methods([
            salvo::http::Method::GET,
            salvo::http::Method::POST,
            salvo::http::Method::PUT,
            salvo::http::Method::DELETE,
            salvo::http::Method::OPTIONS,
        ])
        .allow_headers([
            salvo::http::header::AUTHORIZATION,
            salvo::http::header::CONTENT_TYPE,
        ])
        .allow_credentials(true)
        .into_handler()
}
