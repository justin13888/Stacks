use thiserror::Error;

/// Why a delivery failed, in the fixed categories a caller may be told:
/// nothing finer, so the answer cannot say what is listening where.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryFailure {
    /// The receiver's address is one Stacks refuses to deliver to.
    RefusedAddress,
    /// The receiver did not answer in time.
    Timeout,
    /// The receiver answered with a status other than 2xx (a redirect
    /// included: redirects are not followed).
    HttpError,
    /// The receiver could not be reached at all.
    Unreachable,
}

#[derive(Debug, Error)]
pub enum NotificationError {
    #[error("notification channel not found")]
    ChannelNotFound,
    #[error("notification subscription not found")]
    SubscriptionNotFound,
    #[error("the channel is already subscribed to that event")]
    SubscriptionExists,
    /// `detail` is for the server's log: it names the receiver's origin at
    /// most, never its URL, but it still says more than a caller is told.
    #[error("delivery failed: {detail}")]
    DeliveryFailed {
        failure: DeliveryFailure,
        detail: String,
    },
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}
