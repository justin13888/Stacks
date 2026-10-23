use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImportExportError {
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("parse error: {0}")]
    ParseError(String),
    #[error("upload is {size} bytes; the limit is {max}")]
    TooLarge { size: u64, max: u64 },
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
    #[error("internal error: {0}")]
    Internal(String),
}
