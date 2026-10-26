//! Sending a sample notification through a channel, to check that it
//! delivers.
//!
//! The answer says only whether the receiver accepted it and, when it did
//! not, in which fixed category it failed: never its status, its body or the
//! transport error, which would let any signed-in user ask the server what
//! is listening where. The detail goes to the server's log instead.

use kynos::prelude::*;
use sea_orm::DatabaseConnection;
use stacks_core::notifications::{DeliveryFailure, NotificationError, NotificationService};

use super::{ChannelPath, NotificationChannelNotFound};
use crate::problem::Internal;
use crate::security::{Caller, Writer};

/// Why a test delivery failed: the problem's `category` member, published
/// as `refused-address`, `timeout`, `http-error` or `unreachable`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryFailureCategory {
    /// The receiver's address is one Stacks does not deliver to: loopback,
    /// link-local or the like, or a private network the operator has not
    /// allow-listed.
    RefusedAddress,
    /// The receiver did not answer within ten seconds.
    Timeout,
    /// The receiver answered with a status other than 2xx; redirects are not
    /// followed.
    HttpError,
    /// The receiver could not be reached.
    Unreachable,
}

impl DeliveryFailureCategory {
    /// The fixed `detail` of a failure in this category.
    fn sentence(self) -> &'static str {
        match self {
            Self::RefusedAddress => "the receiver's address is not one Stacks delivers to",
            Self::Timeout => "the receiver did not answer in time",
            Self::HttpError => "the receiver did not accept the delivery",
            Self::Unreachable => "the receiver could not be reached",
        }
    }
}

/// The `category` member as it is published.
impl From<DeliveryFailureCategory> for serde_json::Value {
    fn from(category: DeliveryFailureCategory) -> Self {
        Self::String(
            match category {
                DeliveryFailureCategory::RefusedAddress => "refused-address",
                DeliveryFailureCategory::Timeout => "timeout",
                DeliveryFailureCategory::HttpError => "http-error",
                DeliveryFailureCategory::Unreachable => "unreachable",
            }
            .to_owned(),
        )
    }
}

impl From<DeliveryFailure> for DeliveryFailureCategory {
    fn from(failure: DeliveryFailure) -> Self {
        match failure {
            DeliveryFailure::RefusedAddress => Self::RefusedAddress,
            DeliveryFailure::Timeout => Self::Timeout,
            DeliveryFailure::HttpError => Self::HttpError,
            DeliveryFailure::Unreachable => Self::Unreachable,
        }
    }
}

/// What sending a test notification can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
#[problem(base = crate::problem::BASE)]
pub enum TestChannelError {
    #[error(transparent)]
    #[problem(transparent)]
    NotFound(#[from] NotificationChannelNotFound),
    /// The delivery did not reach the receiver, or the receiver refused it;
    /// `category` says which, and nothing more.
    #[error("{}", category.sentence())]
    #[problem(status = 502, title = "Notification delivery failed")]
    NotificationDeliveryFailed {
        /// Why it failed.
        #[problem(extension)]
        category: DeliveryFailureCategory,
    },
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

impl From<NotificationError> for TestChannelError {
    fn from(e: NotificationError) -> Self {
        match e {
            NotificationError::ChannelNotFound => NotificationChannelNotFound.into(),
            NotificationError::DeliveryFailed { failure, .. } => Self::NotificationDeliveryFailed {
                category: failure.into(),
            },
            other => Internal::log(other).into(),
        }
    }
}

/// Send a sample notification through a channel, to check that it delivers.
///
/// The channel is tried whether it is enabled or not, and whatever it is
/// subscribed to. The answer comes once the receiver has accepted the
/// delivery; a receiver that refuses it, sits at an address Stacks does not
/// deliver to, or does not answer within ten seconds, is a
/// `notification-delivery-failed` problem whose `category` says which.
#[kynos::post(
    "/notification-channels/{id}/tests",
    operation_id = "testNotificationChannel"
)]
pub(super) async fn test_notification_channel(
    Auth(caller): Writer,
    Path(path): Path<ChannelPath>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<NoContent, TestChannelError> {
    let caller = Caller::from(caller);
    NotificationService::new(db)
        .send_test(caller.id(), path.id)
        .await?;
    Ok(NoContent)
}
