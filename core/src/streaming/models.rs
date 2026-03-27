use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorLibraryItem {
    pub server_internal_id: String,
    pub title: String,
    pub quality: Option<String>,
    pub deep_link: String,
    pub playback_link: String,
    pub external_ids: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConnectorEvent {
    PlaybackStarted {
        server_internal_id: String,
        user_id: Option<String>,
        position_seconds: Option<i32>,
    },
    PlaybackStopped {
        server_internal_id: String,
        user_id: Option<String>,
        position_seconds: Option<i32>,
    },
    PlaybackPaused {
        server_internal_id: String,
        user_id: Option<String>,
        position_seconds: Option<i32>,
    },
    PlaybackFinished {
        server_internal_id: String,
        user_id: Option<String>,
    },
    LibraryChanged {
        server_internal_id: Option<String>,
    },
    Unknown {
        raw: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorHealth {
    pub healthy: bool,
    pub version: Option<String>,
    pub message: Option<String>,
}
