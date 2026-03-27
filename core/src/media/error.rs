use thiserror::Error;

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("media not found: {0}")]
    NotFound(String),
    #[error("cameo error: {0}")]
    CameoError(String),
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}
