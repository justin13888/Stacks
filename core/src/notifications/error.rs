use thiserror::Error;

#[derive(Debug, Error)]
pub enum NotificationError {
    #[error("notification channel not found")]
    ChannelNotFound,
    #[error("notification subscription not found")]
    SubscriptionNotFound,
    #[error("the channel is already subscribed to that event")]
    SubscriptionExists,
    #[error("delivery failed: {0}")]
    DeliveryFailed(String),
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}
