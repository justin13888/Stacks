//! Where the caller's notifications go: delivery channels, and which events
//! each channel receives.
//!
//! A channel is a webhook or an Apprise endpoint. A subscription routes one
//! event type to one channel; a channel takes each event type at most once.
//! Secrets a channel holds (a webhook's signing secret, its headers' values)
//! are written, never read back: a channel says only whether it has one.

use std::collections::BTreeMap;

use kynos::extract::link::Link;
use kynos::prelude::*;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use stacks_core::entity::enums::{ChannelType, NotificationEventType};
use stacks_core::entity::{notification_channel, notification_subscription};
use stacks_core::notifications::channels::{AppriseChannelConfig, WebhookChannelConfig};
use stacks_core::notifications::{
    ChannelUpdate, ConfigMerge, NewChannel, NotificationError, NotificationService,
};
use uuid::Uuid;

use crate::app::App;
use crate::page::{self, InvalidCursor, Page, PageParams};
use crate::patch::Nullable;
use crate::problem::Internal;
use crate::security::{Caller, Reader, Writer};

/// The caller's notification routing.
#[derive(Tag)]
#[tag(
    name = "notifications",
    description = "Delivery channels (webhooks, Apprise) and the events each one receives."
)]
pub struct Notifications;

// ------------------------------------------------------------------ channels

/// The method a webhook is delivered with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Schema, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum WebhookMethod {
    #[default]
    Post,
    Put,
    Patch,
}

impl WebhookMethod {
    fn as_str(self) -> &'static str {
        match self {
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "POST" => Some(Self::Post),
            "PUT" => Some(Self::Put),
            "PATCH" => Some(Self::Patch),
            _ => None,
        }
    }
}

/// A channel's configuration as written: what it delivers to, by type.
#[derive(Clone, Debug, Schema, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum NewChannelConfig {
    /// A JSON `POST` (or `PUT`, `PATCH`) of each notification to a URL.
    Webhook {
        /// An absolute `http` or `https` URL.
        #[schema(min_length = 1)]
        url: String,
        /// `POST` when left out.
        method: Option<WebhookMethod>,
        /// Extra headers sent with every delivery, such as an
        /// `Authorization` the receiver expects.
        headers: Option<BTreeMap<String, String>>,
        /// When set, each delivery's body is signed with HMAC-SHA256 under
        /// this secret, the hex digest sent as `X-Stacks-Signature`.
        #[schema(min_length = 1)]
        secret: Option<String>,
    },
    /// An Apprise API notify endpoint.
    Apprise {
        /// An absolute `http` or `https` URL, such as
        /// `https://apprise.example.com/notify/{key}`.
        #[schema(min_length = 1)]
        url: String,
        /// Apprise tags to notify; all of the key's when left out.
        tags: Option<Vec<String>>,
    },
}

impl NewChannelConfig {
    fn into_core(self) -> (ChannelType, serde_json::Value) {
        match self {
            Self::Webhook {
                url,
                method,
                headers,
                secret,
            } => (
                ChannelType::Webhook,
                json_of(&WebhookChannelConfig {
                    url,
                    secret,
                    method: method.map(|m| m.as_str().to_owned()),
                    headers,
                }),
            ),
            Self::Apprise { url, tags } => (
                ChannelType::Apprise,
                json_of(&AppriseChannelConfig { url, tags }),
            ),
        }
    }
}

/// A struct of strings, maps and lists always serializes.
fn json_of<T: Serialize>(value: &T) -> serde_json::Value {
    serde_json::to_value(value).unwrap_or(serde_json::Value::Null)
}

/// A channel's configuration as read back: its secrets are not shown.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChannelConfig {
    Webhook {
        url: String,
        method: WebhookMethod,
        /// The names of the extra headers sent; their values are not shown.
        header_names: Vec<String>,
        /// Whether deliveries are signed.
        has_secret: bool,
    },
    Apprise {
        url: String,
        /// The Apprise tags notified; empty for all of the key's.
        tags: Vec<String>,
    },
}

impl ChannelConfig {
    /// The view of a stored configuration, or `None` (logged) for one that
    /// no longer reads, which validation on every write rules out.
    fn from_stored(channel: &notification_channel::Model) -> Option<Self> {
        let read =
            match channel.channel_type {
                ChannelType::Webhook => serde_json::from_value::<WebhookChannelConfig>(
                    channel.config.clone(),
                )
                .map(|c| Self::Webhook {
                    url: c.url,
                    method: c
                        .method
                        .as_deref()
                        .and_then(WebhookMethod::parse)
                        .unwrap_or_default(),
                    header_names: c.headers.unwrap_or_default().into_keys().collect(),
                    has_secret: c.secret.is_some(),
                }),
                ChannelType::Apprise => serde_json::from_value::<AppriseChannelConfig>(
                    channel.config.clone(),
                )
                .map(|c| Self::Apprise {
                    url: c.url,
                    tags: c.tags.unwrap_or_default(),
                }),
            };
        read.map_err(|e| {
            tracing::warn!(channel_id = %channel.id, error = %e, "unreadable channel config");
        })
        .ok()
    }
}

/// A delivery channel.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct Channel {
    pub id: Uuid,
    pub name: String,
    /// A disabled channel receives nothing, whatever it is subscribed to.
    pub enabled: bool,
    pub config: ChannelConfig,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl Channel {
    fn from_model(c: &notification_channel::Model) -> Result<Self, Internal> {
        let config = ChannelConfig::from_stored(c)
            .ok_or_else(|| Internal::log(format!("channel {} has an unreadable config", c.id)))?;
        Ok(Self {
            id: c.id,
            name: c.name.clone(),
            enabled: c.enabled,
            config,
            created_at: c.created_at.to_utc(),
            updated_at: c.updated_at.to_utc(),
        })
    }
}

/// A channel to create.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct NewChannelBody {
    #[schema(min_length = 1, max_length = 100)]
    pub name: String,
    /// `true` when left out.
    pub enabled: Option<bool>,
    pub config: NewChannelConfig,
}

/// A change to a channel's configuration. Its `type` must be the channel's
/// own; members left out are unchanged, and `null` clears an optional one.
#[derive(Clone, Debug, Schema, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChannelConfigPatch {
    Webhook {
        #[schema(min_length = 1)]
        url: Option<String>,
        #[serde(default)]
        method: Nullable<WebhookMethod>,
        /// Replaces every extra header; `null` removes them all.
        #[serde(default)]
        headers: Nullable<BTreeMap<String, String>>,
        /// `null` stops signing deliveries.
        #[serde(default)]
        secret: Nullable<String>,
    },
    Apprise {
        #[schema(min_length = 1)]
        url: Option<String>,
        #[serde(default)]
        tags: Nullable<Vec<String>>,
    },
}

/// The merge a patch member makes: absent leaves it, `null` removes it.
fn merge_member<T: Serialize>(
    members: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    member: Nullable<T>,
) {
    match member {
        Nullable::Absent => {}
        Nullable::Null => {
            members.insert(key.to_owned(), serde_json::Value::Null);
        }
        Nullable::Value(v) => {
            members.insert(key.to_owned(), json_of(&v));
        }
    }
}

impl ChannelConfigPatch {
    fn into_core(self) -> ConfigMerge {
        let mut members = serde_json::Map::new();
        let channel_type = match self {
            Self::Webhook {
                url,
                method,
                headers,
                secret,
            } => {
                merge_member(
                    &mut members,
                    "url",
                    url.map_or(Nullable::Absent, Nullable::Value),
                );
                merge_member(&mut members, "method", method.map(WebhookMethod::as_str));
                merge_member(&mut members, "headers", headers);
                merge_member(&mut members, "secret", secret);
                ChannelType::Webhook
            }
            Self::Apprise { url, tags } => {
                merge_member(
                    &mut members,
                    "url",
                    url.map_or(Nullable::Absent, Nullable::Value),
                );
                merge_member(&mut members, "tags", tags);
                ChannelType::Apprise
            }
        };
        ConfigMerge {
            channel_type,
            members,
        }
    }
}

/// A change to a channel. Members left out are unchanged.
#[derive(Debug, Default, Schema, Deserialize)]
pub struct ChannelPatch {
    #[schema(min_length = 1, max_length = 100)]
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub config: Option<ChannelConfigPatch>,
}

/// What `/notification-channels/{id}` captures.
#[derive(Debug, Schema, PathParams)]
pub struct ChannelPath {
    pub id: Uuid,
}

/// No channel of the caller's has that id.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("no notification channel of yours has that id")]
#[problem(base = crate::problem::BASE, status = 404, title = "Notification channel not found")]
pub struct NotificationChannelNotFound;

/// The channel's configuration cannot be delivered: a URL that is not
/// absolute http(s), a header Stacks sets itself, a blank name or tag, or a
/// patch of another channel type.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("{0}")]
#[problem(base = crate::problem::BASE, status = 422, title = "Invalid notification channel")]
pub struct InvalidNotificationChannel(pub String);

/// What listing can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum ListError {
    #[error(transparent)]
    #[problem(transparent)]
    InvalidCursor(#[from] InvalidCursor),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What creating a channel can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum CreateChannelError {
    #[error(transparent)]
    #[problem(transparent)]
    Invalid(#[from] InvalidNotificationChannel),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What reading or deleting a channel can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum ChannelError {
    #[error(transparent)]
    #[problem(transparent)]
    NotFound(#[from] NotificationChannelNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What changing a channel can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum UpdateChannelError {
    #[error(transparent)]
    #[problem(transparent)]
    NotFound(#[from] NotificationChannelNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    Invalid(#[from] InvalidNotificationChannel),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

impl From<NotificationError> for CreateChannelError {
    fn from(e: NotificationError) -> Self {
        match e {
            NotificationError::InvalidConfig(why) => InvalidNotificationChannel(why).into(),
            other => Internal::log(other).into(),
        }
    }
}

impl From<NotificationError> for ChannelError {
    fn from(e: NotificationError) -> Self {
        match e {
            NotificationError::ChannelNotFound => NotificationChannelNotFound.into(),
            other => Internal::log(other).into(),
        }
    }
}

impl From<NotificationError> for UpdateChannelError {
    fn from(e: NotificationError) -> Self {
        match e {
            NotificationError::ChannelNotFound => NotificationChannelNotFound.into(),
            NotificationError::InvalidConfig(why) => InvalidNotificationChannel(why).into(),
            other => Internal::log(other).into(),
        }
    }
}

/// List the caller's notification channels, newest first.
#[kynos::get("/notification-channels", operation_id = "listNotificationChannels")]
async fn list_notification_channels(
    Auth(caller): Reader,
    Query(params): Query<PageParams>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<Json<Page<Channel>>, ListError> {
    let caller = Caller::from(caller);
    let channels = NotificationService::new(db)
        .list_channels(caller.id())
        .await
        .map_err(Internal::log)?;
    let page = page::by_id_desc(channels, |c| c.id, &params)?;
    let items = page
        .items
        .iter()
        .map(Channel::from_model)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(Page::new(items, page.next_cursor)))
}

/// Read one of the caller's notification channels.
#[kynos::get("/notification-channels/{id}", operation_id = "getNotificationChannel")]
async fn get_notification_channel(
    Auth(caller): Reader,
    Path(path): Path<ChannelPath>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<Json<Channel>, ChannelError> {
    let caller = Caller::from(caller);
    let channel = NotificationService::new(db)
        .get_channel(caller.id(), path.id)
        .await?;
    Ok(Json(Channel::from_model(&channel)?))
}

/// Create a notification channel.
#[kynos::post("/notification-channels", operation_id = "createNotificationChannel")]
async fn create_notification_channel(
    Auth(caller): Writer,
    channel_link: Link<get_notification_channel>,
    Inject(db): Inject<DatabaseConnection>,
    Json(body): Json<NewChannelBody>,
) -> Result<Created<Json<Channel>>, CreateChannelError> {
    let caller = Caller::from(caller);
    let (channel_type, config) = body.config.into_core();
    let channel = NotificationService::new(db)
        .create_channel(
            caller.id(),
            NewChannel {
                channel_type,
                name: body.name,
                enabled: body.enabled.unwrap_or(true),
                config,
            },
        )
        .await?;
    let location = channel_link.uri(ChannelPath { id: channel.id });
    Ok(Created::at(location, Json(Channel::from_model(&channel)?)))
}

/// Change a notification channel.
///
/// Merge semantics: a member left out is unchanged, and the configuration
/// merges member by member. A channel's type cannot change.
#[kynos::patch(
    "/notification-channels/{id}",
    operation_id = "updateNotificationChannel"
)]
async fn update_notification_channel(
    Auth(caller): Writer,
    Path(path): Path<ChannelPath>,
    Inject(db): Inject<DatabaseConnection>,
    Json(patch): Json<ChannelPatch>,
) -> Result<Json<Channel>, UpdateChannelError> {
    let caller = Caller::from(caller);
    let channel = NotificationService::new(db)
        .update_channel(
            caller.id(),
            path.id,
            ChannelUpdate {
                name: patch.name,
                enabled: patch.enabled,
                config: patch.config.map(ChannelConfigPatch::into_core),
            },
        )
        .await?;
    Ok(Json(Channel::from_model(&channel)?))
}

/// Delete a notification channel and every subscription routing to it.
#[kynos::delete(
    "/notification-channels/{id}",
    operation_id = "deleteNotificationChannel"
)]
async fn delete_notification_channel(
    Auth(caller): Writer,
    Path(path): Path<ChannelPath>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<NoContent, ChannelError> {
    let caller = Caller::from(caller);
    NotificationService::new(db)
        .delete_channel(caller.id(), path.id)
        .await?;
    Ok(NoContent)
}

// ------------------------------------------------------------- subscriptions

/// An event a channel can be notified of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationEvent {
    /// A new episode of a show in the library aired.
    NewEpisode,
    /// A title in the library releases soon.
    UpcomingRelease,
    /// An import finished.
    ImportCompleted,
}

impl From<NotificationEventType> for NotificationEvent {
    fn from(e: NotificationEventType) -> Self {
        match e {
            NotificationEventType::NewEpisode => Self::NewEpisode,
            NotificationEventType::UpcomingRelease => Self::UpcomingRelease,
            NotificationEventType::ImportCompleted => Self::ImportCompleted,
        }
    }
}

impl From<NotificationEvent> for NotificationEventType {
    fn from(e: NotificationEvent) -> Self {
        match e {
            NotificationEvent::NewEpisode => Self::NewEpisode,
            NotificationEvent::UpcomingRelease => Self::UpcomingRelease,
            NotificationEvent::ImportCompleted => Self::ImportCompleted,
        }
    }
}

/// One event type routed to one channel.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct Subscription {
    pub id: Uuid,
    pub channel_id: Uuid,
    pub event: NotificationEvent,
    /// A disabled subscription delivers nothing.
    pub enabled: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<&notification_subscription::Model> for Subscription {
    fn from(s: &notification_subscription::Model) -> Self {
        Self {
            id: s.id,
            channel_id: s.channel_id,
            event: s.event_type.into(),
            enabled: s.enabled,
            created_at: s.created_at.to_utc(),
            updated_at: s.updated_at.to_utc(),
        }
    }
}

/// A subscription to create.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct NewSubscription {
    /// One of the caller's channels.
    pub channel_id: Uuid,
    pub event: NotificationEvent,
    /// `true` when left out.
    pub enabled: Option<bool>,
}

/// A change to a subscription. Members left out are unchanged.
#[derive(Debug, Default, Schema, Serialize, Deserialize)]
pub struct SubscriptionPatch {
    pub enabled: Option<bool>,
}

/// What `/notification-subscriptions/{id}` captures.
#[derive(Debug, Schema, PathParams)]
pub struct SubscriptionPath {
    pub id: Uuid,
}

/// No subscription of the caller's has that id.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("no notification subscription of yours has that id")]
#[problem(base = crate::problem::BASE, status = 404, title = "Notification subscription not found")]
pub struct NotificationSubscriptionNotFound;

/// What creating a subscription can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
#[problem(base = crate::problem::BASE)]
pub enum CreateSubscriptionError {
    #[error(transparent)]
    #[problem(transparent)]
    ChannelNotFound(#[from] NotificationChannelNotFound),
    /// The channel already receives that event.
    #[error("the channel is already subscribed to that event")]
    #[problem(status = 409, title = "Subscription exists")]
    SubscriptionExists,
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What changing or deleting a subscription can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum SubscriptionError {
    #[error(transparent)]
    #[problem(transparent)]
    NotFound(#[from] NotificationSubscriptionNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

impl From<NotificationError> for SubscriptionError {
    fn from(e: NotificationError) -> Self {
        match e {
            NotificationError::SubscriptionNotFound => NotificationSubscriptionNotFound.into(),
            other => Internal::log(other).into(),
        }
    }
}

/// List the caller's notification subscriptions, newest first.
#[kynos::get(
    "/notification-subscriptions",
    operation_id = "listNotificationSubscriptions"
)]
async fn list_notification_subscriptions(
    Auth(caller): Reader,
    Query(params): Query<PageParams>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<Json<Page<Subscription>>, ListError> {
    let caller = Caller::from(caller);
    let subscriptions = NotificationService::new(db)
        .list_subscriptions(caller.id())
        .await
        .map_err(Internal::log)?;
    let page = page::by_id_desc(subscriptions, |s| s.id, &params)?;
    Ok(Json(page.map(|s| Subscription::from(&s))))
}

/// Change a notification subscription.
#[kynos::patch(
    "/notification-subscriptions/{id}",
    operation_id = "updateNotificationSubscription"
)]
async fn update_notification_subscription(
    Auth(caller): Writer,
    Path(path): Path<SubscriptionPath>,
    Inject(db): Inject<DatabaseConnection>,
    Json(patch): Json<SubscriptionPatch>,
) -> Result<Json<Subscription>, SubscriptionError> {
    let caller = Caller::from(caller);
    let subscription = NotificationService::new(db)
        .update_subscription(caller.id(), path.id, patch.enabled)
        .await?;
    Ok(Json(Subscription::from(&subscription)))
}

/// Route an event type to one of the caller's channels.
#[kynos::post(
    "/notification-subscriptions",
    operation_id = "createNotificationSubscription"
)]
async fn create_notification_subscription(
    Auth(caller): Writer,
    subscription_link: Link<update_notification_subscription>,
    Inject(db): Inject<DatabaseConnection>,
    Json(body): Json<NewSubscription>,
) -> Result<Created<Json<Subscription>>, CreateSubscriptionError> {
    let caller = Caller::from(caller);
    let subscription = NotificationService::new(db)
        .create_subscription(
            caller.id(),
            body.channel_id,
            body.event.into(),
            body.enabled.unwrap_or(true),
        )
        .await
        .map_err(|e| match e {
            NotificationError::ChannelNotFound => NotificationChannelNotFound.into(),
            NotificationError::SubscriptionExists => CreateSubscriptionError::SubscriptionExists,
            other => Internal::log(other).into(),
        })?;
    let location = subscription_link.uri(SubscriptionPath {
        id: subscription.id,
    });
    Ok(Created::at(
        location,
        Json(Subscription::from(&subscription)),
    ))
}

/// Delete a notification subscription.
#[kynos::delete(
    "/notification-subscriptions/{id}",
    operation_id = "deleteNotificationSubscription"
)]
async fn delete_notification_subscription(
    Auth(caller): Writer,
    Path(path): Path<SubscriptionPath>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<NoContent, SubscriptionError> {
    let caller = Caller::from(caller);
    NotificationService::new(db)
        .delete_subscription(caller.id(), path.id)
        .await?;
    Ok(NoContent)
}

pub(crate) fn group() -> Group<App> {
    Group::new("/").tag::<Notifications>().mount(kynos::routes![
        list_notification_channels,
        get_notification_channel,
        create_notification_channel,
        update_notification_channel,
        delete_notification_channel,
        list_notification_subscriptions,
        create_notification_subscription,
        update_notification_subscription,
        delete_notification_subscription,
    ])
}
