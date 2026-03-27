use thiserror::Error;

#[derive(Debug, Error)]
pub enum NotificationError {
    #[error("notification channel not found")]
    ChannelNotFound,
    #[error("unsupported channel type: {0}")]
    UnsupportedChannelType(String),
    #[error("delivery failed: {0}")]
    DeliveryFailed(String),
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}
