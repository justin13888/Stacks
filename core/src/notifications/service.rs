use chrono::Utc;
use sea_orm::{ActiveValue::Set, DatabaseConnection};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::entity::{notification_channel, notification_preference};

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

    /// Dispatch a notification event to all enabled channels for matched users.
    pub async fn dispatch(
        &self,
        _user_id: &str,
        _event: NotificationEvent,
    ) -> Result<(), NotificationError> {
        todo!("notification dispatch requires cameo integration for content resolution")
    }
}
