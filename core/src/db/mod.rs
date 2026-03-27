pub mod pool;

pub use pool::connect;

use sea_orm::DatabaseConnection;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("database connection error: {0}")]
    Connection(#[from] sea_orm::DbErr),
}

pub type Db = DatabaseConnection;
