use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};
use stacks_core::media::models::AvailabilityInfo;

use crate::media::dto::AvailabilityServer;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AvailabilityResponse {
    pub available: bool,
    pub servers: Vec<AvailabilityServer>,
}

impl From<AvailabilityInfo> for AvailabilityResponse {
    fn from(a: AvailabilityInfo) -> Self {
        Self {
            available: a.available,
            servers: a
                .servers
                .into_iter()
                .map(AvailabilityServer::from)
                .collect(),
        }
    }
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
