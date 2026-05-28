use thiserror::Error;

#[derive(Debug, Error)]
pub enum StreamingError {
    #[error("streaming server not found")]
    ServerNotFound,
    #[error("authentication failed")]
    AuthFailed,
    #[error("upstream request failed: {0}")]
    Upstream(String),
    #[error("webhook signature invalid")]
    InvalidWebhookSignature,
    #[error("unsupported server type: {0}")]
    UnsupportedServerType(String),
    #[error("malformed payload: {0}")]
    Payload(String),
    #[error("crypto error: {0}")]
    Crypto(#[from] crate::crypto::CryptoError),
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
}
