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
use kynos::http::{Request, Response};
use kynos::middleware::Observer;
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
use kynos::router::operation::Route;
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
        // client-side half of the same defence. A front end served from
        // another origin is `cross-site` to the browser, so the origins CORS
        // admits are trusted here too, or a split-origin deployment could
        // read but never write.
        .intercept(
            settings
                .cors_allowed_origins
                .iter()
                .fold(Csrf::new(), |csrf, origin| {
                    csrf.trusting_origin(origin.clone())
                }),
        )
        .intercept(RateLimit::new(
            Quotas::new(ByClientAddress, MemoryCounters::default()).quota(Quota::new(
                "per-client",
                settings.rate_limit_per_minute.max(1),
                Duration::from_secs(60),
            )),
        ))
        .group(crate::sessions::group())
        .group(crate::account::group())
        .group(crate::catalog::group())
        .group(crate::library::group())
        .group(crate::lists::group())
        .group(crate::tags::group())
        .group(crate::blocklist::group())
        .group(crate::imports::group(settings))
        .group(crate::exports::group())
        .group(crate::stats::group())
        .group(crate::notifications::group())
        .group(crate::admin::group());

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
        .observe(RequestLog::default())
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

/// What a request that matched no operation logs instead of a `paths` key,
/// as [`Trace`] does.
const UNMATCHED: &str = "<unmatched>";

/// [`Trace`]'s two events per operation, without the empty `request_id` its
/// opening one carries.
///
/// Observers run before any interceptor, so a request arrives before
/// [`RequestId`] assigns its identifier, and Stacks never echoes a client's.
/// `Trace` would log `"request_id":""` on every `request received`; this
/// leaves the field out. The closing `response sent` event is `Trace`'s own
/// and carries the identifier the response does.
#[derive(Clone, Debug, Default)]
struct RequestLog(Trace);

impl<C> Observer<C> for RequestLog {
    fn on_request(&self, request: &Request, route: Option<Route<'_>>, _context: &C) {
        tracing::info!(
            method = %request.method(),
            matched_path = route.map_or(UNMATCHED, |route| route.path()),
            operation_id = route.map_or(UNMATCHED, |route| route.operation_id()),
            "request received",
        );
    }

    fn on_response(&self, response: &Response, route: Option<Route<'_>>, elapsed: Duration) {
        <Trace as Observer<C>>::on_response(&self.0, response, route, elapsed);
    }

    fn on_panic(&self, payload: &(dyn std::any::Any + Send), route: Option<Route<'_>>) {
        <Trace as Observer<C>>::on_panic(&self.0, payload, route);
    }
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

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::Mutex;

    use kynos::http::HeaderValue;
    use kynos::http::body::Body;
    use serde_json::Value;

    use super::*;

    /// Everything a subscriber writes, kept for the assertions.
    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl Write for Captured {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
        type Writer = Self;

        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    #[test]
    fn only_the_response_event_carries_a_request_id() {
        let logs = Captured::default();
        let subscriber = tracing_subscriber::fmt()
            .json()
            .with_writer(logs.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            let log = RequestLog::default();
            Observer::<()>::on_request(&log, &Request::new(Body::empty()), None, &());
            let mut response = Response::new(Body::empty());
            response
                .headers_mut()
                .insert("x-request-id", HeaderValue::from_static("rid-1"));
            Observer::<()>::on_response(&log, &response, None, Duration::from_millis(1));
        });

        let written = logs.0.lock().unwrap().clone();
        let events: Vec<Value> = String::from_utf8(written)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(events.len(), 2, "{events:?}");
        let (received, sent) = (&events[0]["fields"], &events[1]["fields"]);
        assert_eq!(received["message"], "request received");
        assert_eq!(received["method"], "GET");
        assert_eq!(received["operation_id"], UNMATCHED);
        assert!(received.get("request_id").is_none(), "{received}");
        assert_eq!(sent["message"], "response sent");
        assert_eq!(sent["request_id"], "rid-1");
    }
}
