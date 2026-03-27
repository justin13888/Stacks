use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImportExportError {
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("parse error: {0}")]
    ParseError(String),
    #[error("media not found for external ID: {0}")]
    MediaNotFound(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}
