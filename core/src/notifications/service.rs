use cameo::{CameoClient, DetailProvider};
use chrono::Utc;
use sea_orm::{ActiveValue::Set, DatabaseConnection};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::entity::{notification_channel, notification_preference};
use crate::media::id::{MediaId, MediaKind};

use super::channels::{AppriseChannel, NotificationChannel, RenderedNotification, WebhookChannel};
use super::{NotificationError, repo};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationEvent {
    NewEpisode {
        cameo_media_id: String,
        episode_id: String,
    },
    AvailableOnBeam {
        cameo_media_id: String,
        server_id: String,
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
    pub fn event_type(&self) -> &'static str {
        match self {
            NotificationEvent::NewEpisode { .. } => "new_episode",
            NotificationEvent::AvailableOnBeam { .. } => "available_on_beam",
            NotificationEvent::UpcomingRelease { .. } => "upcoming_release",
            NotificationEvent::ImportCompleted { .. } => "import_completed",
        }
    }
}

pub struct NotificationService {
    pub db: DatabaseConnection,
}

impl NotificationService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn create_channel(
        &self,
        user_id: &str,
        channel_type: String,
        name: String,
        config: serde_json::Value,
    ) -> Result<notification_channel::Model, NotificationError> {
        let model = notification_channel::ActiveModel {
            id: Set(Ulid::new().to_string()),
            user_id: Set(user_id.to_string()),
            channel_type: Set(channel_type),
            name: Set(name),
            config: Set(config),
            enabled: Set(true),
            created_at: Set(Utc::now().into()),
        };
        repo::create_channel(&self.db, model).await
    }

    pub async fn get_channel(
        &self,
        user_id: &str,
        channel_id: &str,
    ) -> Result<notification_channel::Model, NotificationError> {
        let ch = repo::find_channel_by_id(&self.db, channel_id)
            .await?
            .ok_or(NotificationError::ChannelNotFound)?;
        if ch.user_id != user_id {
            return Err(NotificationError::ChannelNotFound);
        }
        Ok(ch)
    }

    pub async fn list_channels(
        &self,
        user_id: &str,
    ) -> Result<Vec<notification_channel::Model>, NotificationError> {
        repo::find_channels_for_user(&self.db, user_id).await
    }

    pub async fn update_channel(
        &self,
        user_id: &str,
        channel_id: &str,
        name: Option<String>,
        config: Option<serde_json::Value>,
        enabled: Option<bool>,
    ) -> Result<notification_channel::Model, NotificationError> {
        let ch = self.get_channel(user_id, channel_id).await?;
        let mut active: notification_channel::ActiveModel = ch.into();
        if let Some(n) = name {
            active.name = Set(n);
        }
        if let Some(c) = config {
            active.config = Set(c);
        }
        if let Some(e) = enabled {
            active.enabled = Set(e);
        }
        repo::update_channel(&self.db, active).await
    }

    pub async fn delete_channel(
        &self,
        user_id: &str,
        channel_id: &str,
    ) -> Result<(), NotificationError> {
        self.get_channel(user_id, channel_id).await?;
        repo::delete_channel(&self.db, channel_id).await.map(|_| ())
    }

    pub async fn set_preference(
        &self,
        user_id: &str,
        event_type: &str,
        channel_id: &str,
        enabled: bool,
        config: Option<serde_json::Value>,
    ) -> Result<notification_preference::Model, NotificationError> {
        let model = notification_preference::ActiveModel {
            user_id: Set(user_id.to_string()),
            event_type: Set(event_type.to_string()),
            channel_id: Set(channel_id.to_string()),
            enabled: Set(enabled),
            config: Set(config),
        };
        repo::upsert_preference(&self.db, model).await
    }

    pub async fn get_preferences(
        &self,
        user_id: &str,
    ) -> Result<Vec<notification_preference::Model>, NotificationError> {
        repo::find_preferences_for_user(&self.db, user_id).await
    }

    pub async fn delete_preference(
        &self,
        user_id: &str,
        event_type: &str,
        channel_id: &str,
    ) -> Result<(), NotificationError> {
        repo::delete_preference(&self.db, user_id, event_type, channel_id)
            .await
            .map(|_| ())
    }

    /// Dispatch an event to every enabled channel of the user subscribed to
    /// this event type.
    ///
    /// Per-channel delivery failures are logged but do not abort the dispatch
    /// — one broken Apprise endpoint must not silently swallow a webhook.
    pub async fn dispatch(
        &self,
        cameo: &CameoClient,
        user_id: &str,
        event: NotificationEvent,
    ) -> Result<(), NotificationError> {
        let prefs = repo::find_preferences_for_event(&self.db, user_id, event.event_type()).await?;
        if prefs.is_empty() {
            return Ok(());
        }

        let title = resolve_title(cameo, &event).await;
        let rendered = render(&event, title);

        for pref in prefs.into_iter().filter(|p| p.enabled) {
            let channel = match self.get_channel(user_id, &pref.channel_id).await {
                Ok(c) if c.enabled => c,
                Ok(_) => continue,
                Err(e) => {
                    tracing::warn!(channel_id = %pref.channel_id, error = %e, "channel lookup failed");
                    continue;
                }
            };
            let delivery = match channel.channel_type.as_str() {
                "webhook" => WebhookChannel::new().send(&rendered, &channel.config).await,
                "apprise" => AppriseChannel::new().send(&rendered, &channel.config).await,
                other => Err(NotificationError::UnsupportedChannelType(other.to_string())),
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

/// Best-effort cameo lookup of the title for an event's `cameo_media_id`.
async fn resolve_title(cameo: &CameoClient, event: &NotificationEvent) -> Option<String> {
    let raw = match event {
        NotificationEvent::NewEpisode { cameo_media_id, .. }
        | NotificationEvent::AvailableOnBeam { cameo_media_id, .. }
        | NotificationEvent::UpcomingRelease { cameo_media_id, .. } => cameo_media_id.as_str(),
        NotificationEvent::ImportCompleted { .. } => return None,
    };
    let id = MediaId::parse(raw).ok()?;
    match id.kind {
        MediaKind::Movie => cameo
            .movie_details(id.numeric)
            .await
            .ok()
            .map(|d| d.movie.title),
        MediaKind::Tv => cameo
            .tv_show_details(id.numeric)
            .await
            .ok()
            .map(|d| d.show.name),
    }
}

/// Render an event into a human-readable title/body pair, falling back to a
/// generic descriptor when the title couldn't be resolved.
fn render(event: &NotificationEvent, title: Option<String>) -> RenderedNotification {
    let event_type = event.event_type().to_string();
    let item = title.as_deref().unwrap_or("an item in your library");
    let (head, body) = match event {
        NotificationEvent::NewEpisode { episode_id, .. } => (
            format!("New episode of {item}"),
            format!("Episode {episode_id} is now available"),
        ),
        NotificationEvent::AvailableOnBeam { server_id, .. } => (
            format!("{item} is now on Beam"),
            format!("Available on server {server_id}"),
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
