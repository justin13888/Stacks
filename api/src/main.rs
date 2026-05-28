use std::sync::Arc;
use std::time::Duration;

use api::app;
use color_eyre::eyre::{Result, eyre};
use metrics_exporter_prometheus::PrometheusBuilder;
use salvo::Listener;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use stacks_core::entity::streaming_server;
use stacks_core::jobs::{JobContext, JobRunner, repo as jobs_repo};
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

    let cameo = Arc::new(stacks_core::media::build_cameo_client(&config)?);
    let encryption_key = stacks_core::crypto::derive_key(&config.auth.secret_key);

    // Install the Prometheus recorder once at startup; the handle is shared
    // through AppState so the `/metrics` route can render the text format.
    let metrics_handle = PrometheusBuilder::new()
        .install_recorder()
        .map_err(|e| eyre!("failed to install Prometheus recorder: {e}"))?;
    let metrics_handle = Arc::new(metrics_handle);

    // Spawn the background job runner.
    let runner_ctx = JobContext {
        db: db.clone(),
        cameo: cameo.clone(),
        encryption_key,
    };
    tokio::spawn(JobRunner::new(runner_ctx).run(Duration::from_secs(10)));

    // Spawn the periodic Beam-sync scheduler.
    {
        let db_sched = db.clone();
        let interval = Duration::from_secs(config.beam.sync_interval_secs);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(interval);
            // The first `tick().await` fires immediately; skip it so the first
            // real sync runs one interval after startup, not at boot.
            tick.tick().await;
            loop {
                tick.tick().await;
                if let Err(e) = enqueue_beam_syncs(&db_sched).await {
                    tracing::warn!(error = %e, "beam-sync scheduler tick failed");
                }
            }
        });
    }

    let router = app::build_router(app::AppState {
        db,
        config: config.clone(),
        cameo,
        metrics: Some(metrics_handle),
    });

    let acceptor = salvo::conn::TcpListener::new(&config.server.bind)
        .try_bind()
        .await?;
    salvo::Server::new(acceptor).serve(router).await;

    Ok(())
}

/// Enqueue a `beam_sync` job for every enabled streaming server.
async fn enqueue_beam_syncs(db: &DatabaseConnection) -> Result<()> {
    let servers = streaming_server::Entity::find()
        .filter(streaming_server::Column::Enabled.eq(true))
        .all(db)
        .await?;
    for server in servers {
        let payload = serde_json::json!({ "server_id": server.id });
        let job = jobs_repo::build_job(jobs_repo::JobType::BeamSync, Some(payload), 3);
        if let Err(e) = jobs_repo::enqueue_job(db, job).await {
            tracing::warn!(server_id = %server.id, error = %e, "failed to enqueue beam_sync");
        }
    }
    Ok(())
}
