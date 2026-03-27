use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AvailabilityResponse {
    pub available: bool,
    pub servers: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ServerResponse {
    pub id: String,
    pub name: String,
    pub server_type: String,
    pub enabled: bool,
    pub last_synced_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ServerListResponse {
    pub servers: Vec<ServerResponse>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ServerStatusResponse {
    pub healthy: bool,
    pub message: Option<String>,
}
