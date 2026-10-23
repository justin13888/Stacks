//! Probes and metrics, outside `/api/v1`.
//!
//! `/healthz` and `/readyz` are on the public listener, for the container
//! runtime. `/metrics` is on the operator listener ([`internal_group`]),
//! which has its own document, so the public contract never carries it.

use kynos::error::rejection::AuthRejection;
use kynos::extract::body::binary::Binary;
use kynos::http::media::MediaType;
use kynos::prelude::*;
use kynos::security::carrier::BearerToken;
use kynos::security::{Authenticates, Authenticator, Unscoped, constant_time_eq};
use metrics_exporter_prometheus::PrometheusHandle;
use migration::MigratorTrait;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use stacks_core::media::breaker::BreakerState;

use crate::app::App;

/// Liveness and readiness.
#[derive(Tag)]
#[tag(name = "ops", description = "Liveness and readiness probes.")]
pub struct Ops;

/// Operator-only endpoints.
#[derive(Tag)]
#[tag(
    name = "internal",
    description = "Operator endpoints, served on the metrics listener."
)]
pub struct InternalTag;

/// The process is up.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct Liveness {
    /// Always `ok`.
    pub status: String,
}

/// A metadata provider's circuit breaker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CircuitState {
    /// Calls flow normally.
    Closed,
    /// One probe call is testing whether the provider recovered.
    HalfOpen,
    /// The provider has been failing; calls are refused without a request.
    Open,
}

impl From<BreakerState> for CircuitState {
    fn from(state: BreakerState) -> Self {
        match state {
            BreakerState::Closed => Self::Closed,
            BreakerState::HalfOpen => Self::HalfOpen,
            BreakerState::Open => Self::Open,
        }
    }
}

/// One metadata provider's health.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct ProviderHealth {
    /// `tmdb` or `anilist`.
    pub provider: String,
    pub circuit: CircuitState,
}

/// The service can take traffic.
///
/// Metadata providers are reported and never make the service unready: the
/// library is served from the catalog snapshot while they are down.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct Readiness {
    /// Always `ready` when this is returned.
    pub status: String,
    pub metadata: Vec<ProviderHealth>,
}

/// Why the service cannot take traffic.
#[derive(Debug, thiserror::Error, ApiError)]
#[problem(base = crate::problem::BASE)]
pub enum NotReady {
    #[error("the database is unreachable")]
    #[problem(status = 503, title = "Database unavailable")]
    DatabaseUnavailable,
    #[error("the database schema is behind this release; migrations are pending")]
    #[problem(status = 503, title = "Migrations pending")]
    MigrationsPending,
}

/// Report that the process is alive.
///
/// Answers without touching any dependency, so a slow database never gets
/// the process restarted.
#[kynos::get("/healthz", operation_id = "getLiveness")]
async fn get_liveness() -> Json<Liveness> {
    Json(Liveness {
        status: "ok".to_owned(),
    })
}

/// Report whether the service can take traffic.
///
/// Ready when the database answers and its schema is current.
#[kynos::get("/readyz", operation_id = "getReadiness")]
async fn get_readiness(
    Inject(db): Inject<DatabaseConnection>,
    Inject(metadata): Inject<std::sync::Arc<dyn stacks_core::media::MetadataProvider>>,
) -> Result<Json<Readiness>, NotReady> {
    db.ping().await.map_err(|e| {
        tracing::warn!(error = %e, "readiness: database ping failed");
        NotReady::DatabaseUnavailable
    })?;
    let pending = migration::Migrator::get_pending_migrations_read_only(&db)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "readiness: migration status unavailable");
            NotReady::DatabaseUnavailable
        })?;
    if !pending.is_empty() {
        return Err(NotReady::MigrationsPending);
    }
    let metadata = metadata
        .circuits()
        .into_iter()
        .map(|(provider, state)| ProviderHealth {
            provider: provider.as_str().to_owned(),
            circuit: state.into(),
        })
        .collect();
    Ok(Json(Readiness {
        status: "ready".to_owned(),
        metadata,
    }))
}

pub(crate) fn group() -> Group<App> {
    Group::new("/")
        .tag::<Ops>()
        .mount(kynos::routes![get_liveness, get_readiness])
}

// ------------------------------------------------------------------ metrics

/// The operator listener's context.
#[derive(Provider)]
pub struct Operator {
    pub prometheus: PrometheusHandle,
    /// The bearer token `/metrics` demands; `None` serves it to anyone who
    /// can reach the listener.
    #[provide(skip)]
    pub token: Option<String>,
}

/// The Prometheus text exposition format, version 0.0.4.
#[derive(Clone, Copy, Debug, Default)]
pub struct PrometheusText;

impl MediaType for PrometheusText {
    const MEDIA_TYPE: &'static str = "text/plain; version=0.0.4; charset=utf-8";
}

/// The token a scraper presents, when the deployment configured one.
#[derive(SecurityScheme)]
#[security(bearer, carrier = optional)]
#[security(name = "MetricsToken", credential = (), description = "`WW_METRICS_TOKEN`, when the deployment sets one.")]
pub struct MetricsToken;

pub struct MetricsGate;

impl Authenticator<MetricsToken, Operator> for MetricsGate {
    async fn authenticate(
        &self,
        presented: Option<BearerToken>,
        context: &Operator,
    ) -> Result<(), AuthRejection> {
        let Some(expected) = context.token.as_deref() else {
            return Ok(());
        };
        match presented {
            Some(token) if constant_time_eq(token.as_str().as_bytes(), expected.as_bytes()) => {
                Ok(())
            }
            _ => Err(AuthRejection::unauthenticated()),
        }
    }

    async fn authorize(
        &self,
        _credential: &(),
        _scopes: &[Unscoped],
        _context: &Operator,
    ) -> Result<(), AuthRejection> {
        Ok(())
    }
}

impl Authenticates<MetricsToken> for Operator {
    type Authenticator = MetricsGate;

    fn authenticator(&self) -> &MetricsGate {
        &MetricsGate
    }
}

/// Scrape the service's metrics.
///
/// HTTP requests by operation and status, metadata-provider calls, failures
/// and circuit states, and background job outcomes.
#[kynos::get("/metrics", operation_id = "getMetrics")]
async fn get_metrics(
    _scraper: Auth<MetricsToken>,
    Inject(prometheus): Inject<PrometheusHandle>,
) -> Binary<PrometheusText> {
    Binary::new(prometheus.render())
}

pub(crate) fn internal_group() -> Group<Operator> {
    Group::new("/")
        .tag::<InternalTag>()
        .mount(kynos::routes![get_metrics])
}
