//! The `stacks-api` server.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use api::app::{self, App, Settings};
use api::ops::Operator;
use color_eyre::eyre::{Result, WrapErr, eyre};
use kynos::server::Server;
use kynos::server::shutdown::Shutdown;
use metrics_exporter_prometheus::PrometheusBuilder;
use migration::MigratorTrait;
use stacks_core::auth::AuthService;
use stacks_core::jobs::{JobContext, JobRunner};
use tokio_util::sync::CancellationToken;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    color_eyre::install()?;

    fmt()
        .with_env_filter(
            EnvFilter::try_from_env("WW_LOG_LEVEL").unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .json()
        .init();

    let config = stacks_core::config::load()?;
    info!(bind = %config.server.bind, "starting stacks-api");

    let db = stacks_core::db::connect(&config.database.url).await?;
    migration::Migrator::up(&db, None)
        .await
        .wrap_err("database migration failed")?;

    let metadata = stacks_core::media::build_metadata_provider(&config)
        .map_err(|e| eyre!("metadata provider: {e}"))?;
    let oidc = stacks_core::auth::service::oidc_provider(&config.oidc)
        .map_err(|e| eyre!("OIDC provider: {e}"))?;
    let auth = Arc::new(AuthService::new(
        db.clone(),
        config.auth.clone(),
        Arc::new(oidc),
    ));

    let prometheus = PrometheusBuilder::new()
        .install_recorder()
        .map_err(|e| eyre!("failed to install the Prometheus recorder: {e}"))?;

    // The runner stops claiming as soon as a termination signal arrives,
    // rather than when the API drain ends, and hands its jobs back.
    let stop_jobs = CancellationToken::new();
    tokio::spawn(cancel_on_signal(stop_jobs.clone()));
    let runner = tokio::spawn(
        JobRunner::new(JobContext {
            db: db.clone(),
            metadata: metadata.clone(),
        })
        .run(Duration::from_secs(10), stop_jobs.clone()),
    );

    if !config.server.metrics_bind.trim().is_empty() {
        let addr: SocketAddr = config
            .server
            .metrics_bind
            .parse()
            .wrap_err("WW_METRICS_BIND is not an address")?;
        let operator = Operator {
            prometheus,
            token: config.server.metrics_token.clone(),
        };
        let service = app::internal_router().build(operator)?;
        let server = Server::new(service).bind(addr).prepare().await?;
        info!(addr = %addr, "serving /metrics");
        tokio::spawn(async move {
            if let Err(e) = server.serve().await {
                tracing::error!(error = %e, "the metrics listener stopped");
            }
        });
    }

    let settings = Settings::from_config(&config);
    let addr: SocketAddr = config
        .server
        .bind
        .parse()
        .wrap_err("WW_BIND is not an address")?;
    let service = app::router(&settings).build(App {
        db,
        metadata,
        auth,
        settings: Arc::new(settings),
    })?;
    let server = Server::new(service)
        .bind(addr)
        .graceful_shutdown(Shutdown::signals())
        .shutdown_timeout(Duration::from_secs(20))
        .prepare()
        .await?;
    info!(addr = %addr, "serving the API");
    let served = server.serve().await;
    stop_jobs.cancel();
    if tokio::time::timeout(RUNNER_STOP_TIMEOUT, runner)
        .await
        .is_err()
    {
        tracing::warn!("the job runner did not stop in time; its job is reclaimed later");
    }
    match served {
        // A drain ended by its deadline or a second signal is still the stop
        // this process was asked for.
        Err(kynos::Error::Server(e)) if e.is_requested_shutdown() => {
            info!(reason = %e, "stopped");
            Ok(())
        }
        outcome => Ok(outcome?),
    }
}

/// How long shutdown waits for the job runner to release its jobs.
const RUNNER_STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// Cancel `token` on the first `SIGINT` or `SIGTERM` (Ctrl-C elsewhere): the
/// signals the server's own graceful shutdown listens for.
///
/// A listener that cannot be installed never fires; the runner then stops
/// when `serve` returns.
async fn cancel_on_signal(token: CancellationToken) {
    use std::future::pending;

    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!(error = %e, "cannot listen for Ctrl-C");
            pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                if term.recv().await.is_none() {
                    pending::<()>().await;
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "cannot listen for SIGTERM");
                pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = pending::<()>();
    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
    token.cancel();
}
