pub mod pool;

pub use pool::connect;

use sea_orm::{DatabaseConnection, DbErr, SqlErr};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("database connection error: {0}")]
    Connection(#[from] sea_orm::DbErr),
}

pub type Db = DatabaseConnection;

/// Whether `err` is a unique-constraint violation.
///
/// Matches the SQLSTATE through SeaORM's classification rather than the error
/// text, which varies with the driver and the server's locale.
pub fn is_unique_violation(err: &DbErr) -> bool {
    matches!(err.sql_err(), Some(SqlErr::UniqueConstraintViolation(_)))
}

/// The current time as the `timestamptz` type entities use.
pub fn now() -> sea_orm::prelude::DateTimeWithTimeZone {
    chrono::Utc::now().into()
}
