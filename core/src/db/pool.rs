use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use std::time::Duration;

use super::DbError;

pub async fn connect(database_url: &str) -> Result<DatabaseConnection, DbError> {
    let mut opts = ConnectOptions::new(database_url);
    opts.max_connections(20)
        .min_connections(2)
        .connect_timeout(Duration::from_secs(10))
        .acquire_timeout(Duration::from_secs(10))
        .idle_timeout(Duration::from_secs(600))
        .max_lifetime(Duration::from_secs(1800))
        .sqlx_logging(false);

    let db = Database::connect(opts).await?;
    Ok(db)
}
