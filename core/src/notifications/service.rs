use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use crate::entity::{notification_channel, notification_preference};

use super::NotificationError;

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
        let _ = (user_id, channel_type, name, config);
        todo!()
    }

    pub async fn get_channel(
        &self,
        user_id: &str,
        channel_id: &str,
    ) -> Result<notification_channel::Model, NotificationError> {
        let _ = (user_id, channel_id);
        todo!()
    }

    pub async fn list_channels(
        &self,
        user_id: &str,
    ) -> Result<Vec<notification_channel::Model>, NotificationError> {
        let _ = user_id;
        todo!()
    }

    pub async fn update_channel(
        &self,
        user_id: &str,
        channel_id: &str,
        name: Option<String>,
        config: Option<serde_json::Value>,
        enabled: Option<bool>,
    ) -> Result<notification_channel::Model, NotificationError> {
        let _ = (user_id, channel_id, name, config, enabled);
        todo!()
    }

    pub async fn delete_channel(
        &self,
        user_id: &str,
        channel_id: &str,
    ) -> Result<(), NotificationError> {
        let _ = (user_id, channel_id);
        todo!()
    }

    pub async fn set_preference(
        &self,
        user_id: &str,
        event_type: &str,
        channel_id: &str,
        enabled: bool,
        config: Option<serde_json::Value>,
    ) -> Result<notification_preference::Model, NotificationError> {
        let _ = (user_id, event_type, channel_id, enabled, config);
        todo!()
    }

    pub async fn get_preferences(
        &self,
        user_id: &str,
    ) -> Result<Vec<notification_preference::Model>, NotificationError> {
        let _ = user_id;
        todo!()
    }

    pub async fn delete_preference(
        &self,
        user_id: &str,
        event_type: &str,
        channel_id: &str,
    ) -> Result<(), NotificationError> {
        let _ = (user_id, event_type, channel_id);
        todo!()
    }

    /// Dispatch a notification event to all enabled channels for matched users.
    pub async fn dispatch(
        &self,
        user_id: &str,
        event: NotificationEvent,
    ) -> Result<(), NotificationError> {
        let _ = (user_id, event);
        todo!()
    }
}
