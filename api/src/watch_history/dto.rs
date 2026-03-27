use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct HistoryListResponse {
    pub items: Vec<serde_json::Value>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ManualHistoryRequest {
    pub cameo_media_id: String,
    pub cameo_episode_id: Option<String>,
    pub event: String,
    pub position_seconds: Option<i32>,
    pub timestamp: Option<String>,
}
