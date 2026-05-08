use api::app;
use color_eyre::eyre::Result;
use salvo::Listener;
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

    // Run pending migrations on startup
    use migration::MigratorTrait;
    migration::Migrator::up(&db, None).await?;

    let cameo = stacks_core::cameo::build(&config)?;

    let router = app::build_router(app::AppState {
        db: db.clone(),
        config: config.clone(),
        cameo: cameo.clone(),
    });

    // Spawn the background job runner (imports, beam sync, notifications).
    tokio::spawn(async move {
        let runner = stacks_core::jobs::JobRunner::new(db, cameo);
        loop {
            if let Err(e) = runner.tick().await {
                tracing::warn!(error = %e, "job runner tick failed");
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });

    let acceptor = salvo::conn::TcpListener::new(&config.server.bind)
        .try_bind()
        .await?;
    salvo::Server::new(acceptor).serve(router).await;

    Ok(())
}
