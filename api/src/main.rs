use std::sync::Arc;
use std::time::Duration;

use api::app;
use color_eyre::eyre::{Result, eyre};
use metrics_exporter_prometheus::PrometheusBuilder;
use salvo::Listener;
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

    // Run pending migrations on startup.
    use migration::MigratorTrait;
    migration::Migrator::up(&db, None).await?;

    let metadata = stacks_core::media::build_metadata_provider(&config)?;

    // Install the Prometheus recorder once at startup; the handle is shared
    // through AppState so the `/metrics` route can render the text format.
    let metrics_handle = PrometheusBuilder::new()
        .install_recorder()
        .map_err(|e| eyre!("failed to install Prometheus recorder: {e}"))?;
    let metrics_handle = Arc::new(metrics_handle);

    // Spawn the background job runner.
    let runner_ctx = JobContext {
        db: db.clone(),
        metadata: metadata.clone(),
    };
    tokio::spawn(JobRunner::new(runner_ctx).run(Duration::from_secs(10)));

    let router = app::build_router(app::AppState {
        db,
        config: config.clone(),
        metadata,
        metrics: Some(metrics_handle),
    });

    let acceptor = salvo::conn::TcpListener::new(&config.server.bind)
        .try_bind()
        .await?;
    salvo::Server::new(acceptor).serve(router).await;

    Ok(())
}
