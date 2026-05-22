use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};
use stacks_core::entity::notification_preference;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateChannelRequest {
    pub channel_type: String,
    pub name: String,
    pub config: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateChannelRequest {
    pub name: Option<String>,
    pub config: Option<serde_json::Value>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ChannelResponse {
    pub id: String,
    pub channel_type: String,
    pub name: String,
    pub enabled: bool,
    pub created_at: String,
}

/// One notification preference: whether a given event type is delivered to a
/// given channel.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NotificationPreference {
    pub event_type: String,
    pub channel_id: String,
    pub enabled: bool,
    pub config: Option<serde_json::Value>,
}

impl From<notification_preference::Model> for NotificationPreference {
    fn from(m: notification_preference::Model) -> Self {
        Self {
            event_type: m.event_type,
            channel_id: m.channel_id,
            enabled: m.enabled,
            config: m.config,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PreferencesResponse {
    pub preferences: Vec<NotificationPreference>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdatePreferencesRequest {
    pub preferences: Vec<NotificationPreference>,
}
