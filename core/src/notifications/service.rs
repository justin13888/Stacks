use sea_orm::ActiveValue::{NotSet, Set, Unchanged};
use sea_orm::{DatabaseConnection, TransactionTrait};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::enums::{ChannelType, NotificationEventType};
use crate::entity::{notification_channel, notification_subscription};
use crate::media::catalog;
use crate::media::id::MediaId;

use super::channels::{AppriseChannel, NotificationChannel, RenderedNotification, WebhookChannel};
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

impl NotificationService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn create_channel(
        &self,
        user_id: Uuid,
        channel_type: &str,
        name: String,
        config: serde_json::Value,
    ) -> Result<notification_channel::Model, NotificationError> {
        let channel_type = ChannelType::parse(channel_type)
            .ok_or_else(|| NotificationError::UnsupportedChannelType(channel_type.to_owned()))?;
        if !config.is_object() {
            return Err(NotificationError::InvalidConfig(
                "channel config must be an object".into(),
            ));
        }
        Ok(repo::create_channel(
            &self.db,
            notification_channel::ActiveModel {
                id: Set(Uuid::now_v7()),
                user_id: Set(user_id),
                channel_type: Set(channel_type),
                name: Set(name),
                config: Set(config),
                enabled: Set(true),
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

    /// Merge-patch a channel: absent fields are untouched.
    pub async fn update_channel(
        &self,
        user_id: Uuid,
        channel_id: Uuid,
        name: Option<String>,
        config: Option<serde_json::Value>,
        enabled: Option<bool>,
    ) -> Result<notification_channel::Model, NotificationError> {
        let ch = self.get_channel(user_id, channel_id).await?;
        if config.as_ref().is_some_and(|c| !c.is_object()) {
            return Err(NotificationError::InvalidConfig(
                "channel config must be an object".into(),
            ));
        }
        if name.is_none() && config.is_none() && enabled.is_none() {
            return Ok(ch);
        }
        let am = notification_channel::ActiveModel {
            id: Unchanged(ch.id),
            name: name.map_or(NotSet, Set),
            config: config.map_or(NotSet, Set),
            enabled: enabled.map_or(NotSet, Set),
            ..Default::default()
        };
        Ok(repo::update_channel(&self.db, am).await?)
    }

    /// Delete a channel and its subscriptions, atomically.
    pub async fn delete_channel(
        &self,
        user_id: Uuid,
        channel_id: Uuid,
    ) -> Result<(), NotificationError> {
        self.get_channel(user_id, channel_id).await?;
        let txn = self.db.begin().await?;
        repo::remove_channel(&txn, channel_id).await?;
        txn.commit().await?;
        Ok(())
    }

    /// Subscribe a channel to an event type (or update the subscription).
    pub async fn set_preference(
        &self,
        user_id: Uuid,
        event_type: &str,
        channel_id: Uuid,
        enabled: bool,
        config: Option<serde_json::Value>,
    ) -> Result<notification_subscription::Model, NotificationError> {
        let event_type = parse_event(event_type)?;
        self.get_channel(user_id, channel_id).await?;
        Ok(repo::upsert_subscription(
            &self.db,
            notification_subscription::ActiveModel {
                id: Set(Uuid::now_v7()),
                user_id: Set(user_id),
                channel_id: Set(channel_id),
                event_type: Set(event_type),
                enabled: Set(enabled),
                config: Set(config),
                created_at: NotSet,
                updated_at: NotSet,
            },
        )
        .await?)
    }

    pub async fn get_preferences(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<notification_subscription::Model>, NotificationError> {
        Ok(repo::subscriptions_for_user(&self.db, user_id).await?)
    }

    pub async fn delete_preference(
        &self,
        user_id: Uuid,
        event_type: &str,
        channel_id: Uuid,
    ) -> Result<(), NotificationError> {
        repo::remove_subscription(&self.db, user_id, parse_event(event_type)?, channel_id).await?;
        Ok(())
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

fn parse_event(s: &str) -> Result<NotificationEventType, NotificationError> {
    NotificationEventType::parse(s)
        .ok_or_else(|| NotificationError::InvalidConfig(format!("unknown event type: {s}")))
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
