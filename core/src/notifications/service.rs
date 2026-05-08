use std::sync::Arc;

use cameo::CameoClient;
use cameo::unified::DetailProvider;
use chrono::Utc;
use sea_orm::{ActiveValue::Set, DatabaseConnection};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::cameo::tmdb_numeric_id;
use crate::entity::{notification_channel, notification_preference};

use super::channels::{NotificationChannel, apprise::AppriseChannel, webhook::WebhookChannel};
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

    /// Best-effort cameo media id for the event. Used to hydrate titles when
    /// dispatching the notification.
    pub fn cameo_media_id(&self) -> Option<&str> {
        match self {
            NotificationEvent::NewEpisode { cameo_media_id, .. }
            | NotificationEvent::AvailableOnBeam { cameo_media_id, .. }
            | NotificationEvent::UpcomingRelease { cameo_media_id, .. } => Some(cameo_media_id),
            _ => None,
        }
    }
}

/// Pre-resolved title + summary string passed to channel `send` so each
/// channel doesn't have to know about Cameo. Channels see only the user-facing
/// text and the raw event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedNotification {
    pub title: String,
    pub body: String,
    pub event: NotificationEvent,
}

pub struct NotificationService {
    pub db: DatabaseConnection,
    pub cameo: Arc<CameoClient>,
}

impl NotificationService {
    pub fn new(db: DatabaseConnection, cameo: Arc<CameoClient>) -> Self {
        Self { db, cameo }
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

    /// Dispatch a notification event to all enabled channels for matched users.
    /// Title + body strings are resolved using Cameo so channel
    /// implementations stay simple.
    pub async fn dispatch(
        &self,
        user_id: &str,
        event: NotificationEvent,
    ) -> Result<(), NotificationError> {
        let prefs = repo::find_preferences_for_event(&self.db, user_id, event.event_type()).await?;
        if prefs.is_empty() {
            return Ok(());
        }

        let resolved = self.resolve(event).await;
        for pref in prefs {
            if !pref.enabled {
                continue;
            }
            let Ok(channel) = repo::find_channel_by_id(&self.db, &pref.channel_id).await else {
                continue;
            };
            let Some(channel) = channel else { continue };
            if !channel.enabled {
                continue;
            }
            if let Err(e) = self.send_to_channel(&channel, &resolved).await {
                tracing::warn!(channel_id = %channel.id, error = %e, "notification delivery failed");
            }
        }
        Ok(())
    }

    async fn send_to_channel(
        &self,
        channel: &notification_channel::Model,
        resolved: &ResolvedNotification,
    ) -> Result<(), NotificationError> {
        match channel.channel_type.as_str() {
            "webhook" => {
                let ch = WebhookChannel::new();
                ch.send(resolved, &channel.config).await
            }
            "apprise" => {
                let ch = AppriseChannel::new();
                ch.send(resolved, &channel.config).await
            }
            other => Err(NotificationError::UnsupportedChannelType(other.to_string())),
        }
    }

    async fn resolve(&self, event: NotificationEvent) -> ResolvedNotification {
        let (title, body) = match &event {
            NotificationEvent::NewEpisode { cameo_media_id, episode_id } => {
                let title = self
                    .lookup_title(cameo_media_id)
                    .await
                    .unwrap_or_else(|| cameo_media_id.clone());
                (
                    format!("New episode: {title}"),
                    format!("Episode {episode_id} of {title} is now available."),
                )
            }
            NotificationEvent::AvailableOnBeam {
                cameo_media_id,
                server_id,
            } => {
                let title = self
                    .lookup_title(cameo_media_id)
                    .await
                    .unwrap_or_else(|| cameo_media_id.clone());
                (
                    format!("Available on your server: {title}"),
                    format!("{title} is now playable on server {server_id}."),
                )
            }
            NotificationEvent::UpcomingRelease {
                cameo_media_id,
                release_date,
            } => {
                let title = self
                    .lookup_title(cameo_media_id)
                    .await
                    .unwrap_or_else(|| cameo_media_id.clone());
                (
                    format!("Coming soon: {title}"),
                    format!("{title} releases on {release_date}."),
                )
            }
            NotificationEvent::ImportCompleted {
                total,
                succeeded,
                failed,
                ..
            } => (
                "Import completed".to_string(),
                format!("{succeeded}/{total} entries imported, {failed} failed."),
            ),
        };
        ResolvedNotification { title, body, event }
    }

    async fn lookup_title(&self, cameo_id: &str) -> Option<String> {
        let tmdb = tmdb_numeric_id(cameo_id)?;
        if let Ok(m) = self.cameo.movie_details(tmdb).await {
            return Some(m.movie.title);
        }
        if let Ok(t) = self.cameo.tv_show_details(tmdb).await {
            return Some(t.show.name);
        }
        None
    }
}
