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

    tokio::spawn(
        JobRunner::new(JobContext {
            db: db.clone(),
            metadata: metadata.clone(),
        })
        .run(Duration::from_secs(10)),
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
    match server.serve().await {
        // A drain ended by its deadline or a second signal is still the stop
        // this process was asked for.
        Err(kynos::Error::Server(e)) if e.is_requested_shutdown() => {
            info!(reason = %e, "stopped");
            Ok(())
        }
        outcome => Ok(outcome?),
    }
}
