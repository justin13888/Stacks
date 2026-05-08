use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PreferencesResponse {
    pub preferences: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdatePreferencesRequest {
    pub preferences: Vec<PreferenceEntry>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PreferenceEntry {
    /// Event type — one of `new_episode`, `available_on_beam`,
    /// `upcoming_release`, `import_completed`.
    pub event_type: String,
    pub channel_id: String,
    pub enabled: bool,
    #[serde(default)]
    pub config: Option<serde_json::Value>,
}
