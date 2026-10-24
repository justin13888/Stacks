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

/// The most rows one multi-row `INSERT` carries.
///
/// PostgreSQL's wire protocol allows 65535 bind parameters per statement,
/// so a batch of rows with a dozen columns or fewer fits comfortably.
/// Larger batches are written as several statements in the same transaction.
pub const INSERT_CHUNK: usize = 1000;

/// The current time as the `timestamptz` type entities use, at the
/// microsecond precision `timestamptz` stores, so a value a write returns
/// equals the one later reads of the same row return.
pub fn now() -> sea_orm::prelude::DateTimeWithTimeZone {
    use chrono::SubsecRound;
    chrono::Utc::now().trunc_subsecs(6).into()
}
