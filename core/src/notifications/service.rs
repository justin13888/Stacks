use sea_orm::ActiveValue::{NotSet, Set, Unchanged};
use sea_orm::{DatabaseConnection, SqlErr, TransactionTrait};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::enums::{ChannelType, NotificationEventType};
use crate::entity::{notification_channel, notification_subscription};
use crate::media::catalog;
use crate::media::id::MediaId;

use super::channels::{
    AppriseChannel, AppriseChannelConfig, NotificationChannel, RenderedNotification,
    WebhookChannel, WebhookChannelConfig,
};
use super::{NotificationError, repo};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationEvent {
    NewEpisode {
        cameo_media_id: String,
        episode_id: String,
    },
    UpcomingRelease {
        cameo_media_id: String,
        release_date: String,
    },
    ImportCompleted {
        import_id: String,
        total: u32,
        succeeded: u32,
        failed: u32,
    },
}

impl NotificationEvent {
    pub fn event_type(&self) -> NotificationEventType {
        match self {
            NotificationEvent::NewEpisode { .. } => NotificationEventType::NewEpisode,
            NotificationEvent::UpcomingRelease { .. } => NotificationEventType::UpcomingRelease,
            NotificationEvent::ImportCompleted { .. } => NotificationEventType::ImportCompleted,
        }
    }
}

pub struct NotificationService {
    db: DatabaseConnection,
}

/// A channel to create.
#[derive(Debug, Clone)]
pub struct NewChannel {
    pub channel_type: ChannelType,
    pub name: String,
    pub enabled: bool,
    /// The type's configuration object ([`WebhookChannelConfig`] or
    /// [`AppriseChannelConfig`]).
    pub config: serde_json::Value,
}

/// A merge of members into a channel's stored configuration: a member with a
/// value replaces the stored one, a `null` removes it, and members not named
/// are left alone. `channel_type` must be the channel's own.
#[derive(Debug, Clone)]
pub struct ConfigMerge {
    pub channel_type: ChannelType,
    pub members: serde_json::Map<String, serde_json::Value>,
}

/// A change to a channel; `None` members are untouched.
#[derive(Debug, Clone, Default)]
pub struct ChannelUpdate {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub config: Option<ConfigMerge>,
}

/// Whether `config` is a deliverable configuration for `channel_type`: it
/// reads as the type's config struct (no unknown members) and passes that
/// struct's own checks.
pub fn validate_config(
    channel_type: ChannelType,
    config: &serde_json::Value,
) -> Result<(), NotificationError> {
    let invalid = |e: serde_json::Error| NotificationError::InvalidConfig(e.to_string());
    match channel_type {
        ChannelType::Webhook => serde_json::from_value::<WebhookChannelConfig>(config.clone())
            .map_err(invalid)?
            .validate(),
        ChannelType::Apprise => serde_json::from_value::<AppriseChannelConfig>(config.clone())
            .map_err(invalid)?
            .validate(),
    }
}

fn valid_name(name: String) -> Result<String, NotificationError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(NotificationError::InvalidConfig(
            "the name must not be blank".into(),
        ));
    }
    Ok(trimmed.to_owned())
}

impl NotificationService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Create a channel. Its configuration must be one [`validate_config`]
    /// accepts for its type, and its name must not be blank.
    pub async fn create_channel(
        &self,
        user_id: Uuid,
        new: NewChannel,
    ) -> Result<notification_channel::Model, NotificationError> {
        let name = valid_name(new.name)?;
        validate_config(new.channel_type, &new.config)?;
        Ok(repo::create_channel(
            &self.db,
            notification_channel::ActiveModel {
                id: Set(Uuid::now_v7()),
                user_id: Set(user_id),
                channel_type: Set(new.channel_type),
                name: Set(name),
                config: Set(new.config),
                enabled: Set(new.enabled),
                created_at: NotSet,
                updated_at: NotSet,
            },
        )
        .await?)
    }

    pub async fn get_channel(
        &self,
        user_id: Uuid,
        channel_id: Uuid,
    ) -> Result<notification_channel::Model, NotificationError> {
        repo::find_channel(&self.db, channel_id)
            .await?
            .filter(|c| c.user_id == user_id)
            .ok_or(NotificationError::ChannelNotFound)
    }

    pub async fn list_channels(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<notification_channel::Model>, NotificationError> {
        Ok(repo::channels_for_user(&self.db, user_id).await?)
    }

    /// Merge-patch a channel: absent members are untouched, and the
    /// configuration merges member by member (see [`ConfigMerge`]). The
    /// merged configuration must still be valid; a channel's type never
    /// changes.
    pub async fn update_channel(
        &self,
        user_id: Uuid,
        channel_id: Uuid,
        update: ChannelUpdate,
    ) -> Result<notification_channel::Model, NotificationError> {
        let txn = self.db.begin().await?;
        let ch = repo::find_channel(&txn, channel_id)
            .await?
            .filter(|c| c.user_id == user_id)
            .ok_or(NotificationError::ChannelNotFound)?;
        let name = update.name.map(valid_name).transpose()?;
        let config = match update.config {
            None => None,
            Some(merge) => {
                if merge.channel_type != ch.channel_type {
                    return Err(NotificationError::InvalidConfig(format!(
                        "a {} channel cannot become a {} channel; create a new one",
                        ch.channel_type, merge.channel_type
                    )));
                }
                let mut merged = match ch.config.clone() {
                    serde_json::Value::Object(m) => m,
                    _ => serde_json::Map::new(),
                };
                for (k, v) in merge.members {
                    if v.is_null() {
                        merged.remove(&k);
                    } else {
                        merged.insert(k, v);
                    }
                }
                let merged = serde_json::Value::Object(merged);
                validate_config(ch.channel_type, &merged)?;
                Some(merged)
            }
        };
        if name.is_none() && config.is_none() && update.enabled.is_none() {
            txn.commit().await?;
            return Ok(ch);
        }
        let am = notification_channel::ActiveModel {
            id: Unchanged(ch.id),
            name: name.map_or(NotSet, Set),
            config: config.map_or(NotSet, Set),
            enabled: update.enabled.map_or(NotSet, Set),
            ..Default::default()
        };
        let updated = repo::update_channel(&txn, am).await?;
        txn.commit().await?;
        Ok(updated)
    }

    /// Delete a channel and its subscriptions, atomically.
    pub async fn delete_channel(
        &self,
        user_id: Uuid,
        channel_id: Uuid,
    ) -> Result<(), NotificationError> {
        let txn = self.db.begin().await?;
        repo::find_channel(&txn, channel_id)
            .await?
            .filter(|c| c.user_id == user_id)
            .ok_or(NotificationError::ChannelNotFound)?;
        repo::remove_channel(&txn, channel_id).await?;
        txn.commit().await?;
        Ok(())
    }

    /// Route one event type to one of the user's channels.
    ///
    /// A channel takes each event type once:
    /// [`NotificationError::SubscriptionExists`] when it already does.
    pub async fn create_subscription(
        &self,
        user_id: Uuid,
        channel_id: Uuid,
        event_type: NotificationEventType,
        enabled: bool,
    ) -> Result<notification_subscription::Model, NotificationError> {
        let txn = self.db.begin().await?;
        repo::find_channel(&txn, channel_id)
            .await?
            .filter(|c| c.user_id == user_id)
            .ok_or(NotificationError::ChannelNotFound)?;
        let created = repo::insert_subscription(
            &txn,
            notification_subscription::ActiveModel {
                id: Set(Uuid::now_v7()),
                user_id: Set(user_id),
                channel_id: Set(channel_id),
                event_type: Set(event_type),
                enabled: Set(enabled),
                config: Set(None),
                created_at: NotSet,
                updated_at: NotSet,
            },
        )
        .await
        .map_err(|e| match e.sql_err() {
            Some(SqlErr::UniqueConstraintViolation(_)) => NotificationError::SubscriptionExists,
            _ => NotificationError::Db(e),
        })?;
        txn.commit().await?;
        Ok(created)
    }

    pub async fn list_subscriptions(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<notification_subscription::Model>, NotificationError> {
        Ok(repo::subscriptions_for_user(&self.db, user_id).await?)
    }

    pub async fn get_subscription(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> Result<notification_subscription::Model, NotificationError> {
        repo::find_subscription(&self.db, id)
            .await?
            .filter(|s| s.user_id == user_id)
            .ok_or(NotificationError::SubscriptionNotFound)
    }

    /// Turn a subscription on or off; `None` leaves it as it is.
    pub async fn update_subscription(
        &self,
        user_id: Uuid,
        id: Uuid,
        enabled: Option<bool>,
    ) -> Result<notification_subscription::Model, NotificationError> {
        let sub = self.get_subscription(user_id, id).await?;
        match enabled {
            Some(enabled) if enabled != sub.enabled => Ok(repo::update_subscription(
                &self.db,
                notification_subscription::ActiveModel {
                    id: Unchanged(sub.id),
                    enabled: Set(enabled),
                    ..Default::default()
                },
            )
            .await?),
            _ => Ok(sub),
        }
    }

    pub async fn delete_subscription(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> Result<(), NotificationError> {
        if repo::remove_subscription(&self.db, user_id, id).await? {
            Ok(())
        } else {
            Err(NotificationError::SubscriptionNotFound)
        }
    }

    /// Dispatch an event to every enabled channel subscribed to its type.
    ///
    /// Per-channel delivery failures are logged but do not abort the dispatch
    /// — one broken Apprise endpoint must not silently swallow a webhook.
    pub async fn dispatch(
        &self,
        user_id: Uuid,
        event: NotificationEvent,
    ) -> Result<(), NotificationError> {
        let subs = repo::subscriptions_for_event(&self.db, user_id, event.event_type()).await?;
        if subs.is_empty() {
            return Ok(());
        }

        let title = resolve_title(&self.db, &event).await;
        let rendered = render(&event, title);

        for sub in subs.into_iter().filter(|s| s.enabled) {
            let channel = match self.get_channel(user_id, sub.channel_id).await {
                Ok(c) if c.enabled => c,
                Ok(_) => continue,
                Err(e) => {
                    tracing::warn!(channel_id = %sub.channel_id, error = %e, "channel lookup failed");
                    continue;
                }
            };
            let delivery = match channel.channel_type {
                ChannelType::Webhook => {
                    WebhookChannel::new().send(&rendered, &channel.config).await
                }
                ChannelType::Apprise => {
                    AppriseChannel::new().send(&rendered, &channel.config).await
                }
            };
            if let Err(e) = delivery {
                tracing::warn!(
                    channel_id = %channel.id,
                    channel_type = %channel.channel_type,
                    error = %e,
                    "notification delivery failed"
                );
            }
        }
        Ok(())
    }
}

/// The title for an event's media id, from the catalog snapshot. Events are
/// only raised for titles in the catalog, so no provider call is needed and
/// an upstream outage cannot hold a notification back.
async fn resolve_title(db: &DatabaseConnection, event: &NotificationEvent) -> Option<String> {
    let raw = match event {
        NotificationEvent::NewEpisode { cameo_media_id, .. }
        | NotificationEvent::UpcomingRelease { cameo_media_id, .. } => cameo_media_id.as_str(),
        NotificationEvent::ImportCompleted { .. } => return None,
    };
    let id = MediaId::parse(raw).ok()?;
    match catalog::find_by_ref(db, &id).await {
        Ok(row) => row.map(|m| m.title),
        Err(e) => {
            tracing::warn!(media_id = %id, error = %e, "title lookup failed");
            None
        }
    }
}

/// Render an event into a human-readable title/body pair, falling back to a
/// generic descriptor when the title couldn't be resolved.
fn render(event: &NotificationEvent, title: Option<String>) -> RenderedNotification {
    let event_type = event.event_type().as_str().to_string();
    let item = title.as_deref().unwrap_or("an item in your library");
    let (head, body) = match event {
        NotificationEvent::NewEpisode { episode_id, .. } => (
            format!("New episode of {item}"),
            format!("Episode {episode_id} is now available"),
        ),
        NotificationEvent::UpcomingRelease { release_date, .. } => (
            format!("{item} releases soon"),
            format!("Release date: {release_date}"),
        ),
        NotificationEvent::ImportCompleted {
            import_id,
            total,
            succeeded,
            failed,
        } => (
            "Import complete".to_string(),
            format!("Import {import_id}: {succeeded}/{total} succeeded, {failed} failed"),
        ),
    };
    RenderedNotification {
        event_type,
        title: head,
        body,
    }
}
