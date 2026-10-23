//! The application context and the router every operation hangs off.
//!
//! [`router`] is the whole public API as one value: the OpenAPI document
//! (`gen-openapi`), the served routes (`main`) and every test are built from
//! it, so none of them can describe a route the others do not have.
//! [`internal_router`] is the operator listener (`/metrics`), which runs on its
//! own port so the public contract never carries it.

use std::sync::Arc;
use std::time::Duration;

use kynos::http::forwarded::{ProxyHeader, TrustedProxies};
use kynos::middleware::catch_panic::PanicPolicy;
use kynos::middleware::compression::Compression;
use kynos::middleware::cors::Cors;
use kynos::middleware::csrf::Csrf;
use kynos::middleware::rate_limit::RateLimit;
use kynos::middleware::rate_limit::key::ByClientAddress;
use kynos::middleware::rate_limit::quota::{Quota, Quotas};
use kynos::middleware::request_id::RequestId;
use kynos::middleware::security_headers::SecurityHeaders;
use kynos::middleware::trace::Trace;
use kynos::openapi::{Info, Method};
use kynos::prelude::*;
use kynos::router::docs::Docs;
use kynos::router::policy::FallbackPolicy;
use sea_orm::DatabaseConnection;
use stacks_core::auth::AuthService;
use stacks_core::media::MetadataProvider;

use crate::limits::MemoryCounters;
use crate::observe::Metrics;

/// Everything about a deployment the handlers read, fixed at startup.
#[derive(Clone, Debug)]
pub struct Settings {
    /// Whether the session cookie is `Secure`: true whenever the web app is
    /// served over HTTPS (read from the OIDC redirect URI's scheme).
    pub secure_cookies: bool,
    /// The largest import upload accepted, in bytes.
    pub import_max_bytes: u64,
    /// Origins allowed to call the API with credentials from a browser.
    pub cors_allowed_origins: Vec<String>,
    /// Requests one client may make per minute.
    pub rate_limit_per_minute: u64,
    /// Reverse proxies trusted to report the client address.
    pub trusted_proxy_hops: usize,
}

impl Settings {
    /// The settings a deployment's configuration implies.
    pub fn from_config(config: &stacks_core::config::Config) -> Self {
        Self {
            secure_cookies: config.oidc.redirect_uri.starts_with("https://"),
            import_max_bytes: config.imports.max_file_bytes,
            cors_allowed_origins: config.server.cors_allowed_origins.clone(),
            rate_limit_per_minute: config.server.rate_limit_per_minute,
            trusted_proxy_hops: usize::try_from(config.server.trusted_proxy_hops)
                .unwrap_or(usize::MAX),
        }
    }
}

impl Default for Settings {
    /// What `gen-openapi` describes: the defaults, which change no part of
    /// the document.
    fn default() -> Self {
        Self {
            secure_cookies: true,
            import_max_bytes: 10 * 1024 * 1024,
            cors_allowed_origins: Vec::new(),
            rate_limit_per_minute: 600,
            trusted_proxy_hops: 1,
        }
    }
}

/// The application context every handler draws its dependencies from.
#[derive(Provider)]
pub struct App {
    pub db: DatabaseConnection,
    pub metadata: Arc<dyn MetadataProvider>,
    pub auth: Arc<AuthService>,
    pub settings: Arc<Settings>,
}

/// The document's `info`.
fn info() -> Info {
    Info {
        description: Some(
            "The Stacks API: track movies, shows, anime and Asian dramas. Every operation, \
             schema and status here is derived from the types the server runs on. Errors are \
             RFC 9457 problem documents."
                .to_owned(),
        ),
        ..Info::new("Stacks", env!("CARGO_PKG_VERSION"))
    }
}

/// The public API: `/api/v1`, the probes, and the reference at `/api/docs`.
pub fn router(
    settings: &Settings,
) -> Router<App, impl PanicPolicy + use<>, impl Sized + use<>, impl Sized + use<>> {
    let v1 = Router::<App>::new()
        // Fetch Metadata (Sec-Fetch-Site) refuses a cross-site unsafe request
        // before it reaches the session; SameSite=Lax on the cookie is the
        // client-side half of the same defence.
        .intercept(Csrf::new())
        .intercept(RateLimit::new(
            Quotas::new(ByClientAddress, MemoryCounters::default()).quota(Quota::new(
                "per-client",
                settings.rate_limit_per_minute.max(1),
                Duration::from_secs(60),
            )),
        ))
        .group(crate::sessions::group())
        .group(crate::account::group())
        .group(crate::catalog::group());

    Router::<App>::new()
        .info(info())
        .intercept(SecurityHeaders::new().deny_framing())
        .intercept(
            Cors::new()
                .allow_origins(settings.cors_allowed_origins.iter().cloned())
                .allow_methods([
                    Method::Get,
                    Method::Post,
                    Method::Put,
                    Method::Patch,
                    Method::Delete,
                ])
                .allow_headers(["content-type", "authorization"])
                .allow_credentials()
                .max_age(Duration::from_secs(600)),
        )
        .intercept(RequestId::new())
        .intercept(Compression::new())
        .observe(Trace::new())
        .observe(Metrics)
        .trusted_proxies(TrustedProxies::hops(
            ProxyHeader::XForwarded,
            settings.trusted_proxy_hops,
        ))
        .nest("/api/v1", v1)
        .group(crate::ops::group())
        .docs(
            Docs::scalar()
                .at("/api/docs")
                .description_at("/api/openapi.json"),
        )
        .not_found(FallbackPolicy::Problem)
        .method_not_allowed(FallbackPolicy::Problem)
}

/// The operator listener: `/metrics`, on its own port and in its own
/// document, so the public contract never carries it.
pub fn internal_router() -> Router<crate::ops::Operator, impl PanicPolicy, impl Sized, impl Sized> {
    Router::<crate::ops::Operator>::new()
        .info(Info::new(
            "Stacks operator endpoints",
            env!("CARGO_PKG_VERSION"),
        ))
        .group(crate::ops::internal_group())
        .not_found(FallbackPolicy::Problem)
        .method_not_allowed(FallbackPolicy::Problem)
}
