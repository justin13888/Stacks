use thiserror::Error;

#[derive(Debug, Error)]
pub enum StatsError {
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}
