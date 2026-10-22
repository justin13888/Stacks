use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdminCreateServerRequest {
    pub name: String,
    /// Connector type. Currently only `beam` is supported.
    pub server_type: String,
    pub base_url: String,
    pub api_key: Option<String>,
    /// Shared secret for verifying inbound webhook signatures.
    pub webhook_secret: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdminUpdateServerRequest {
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub webhook_secret: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdminServerResponse {
    pub id: String,
    pub name: String,
    pub server_type: String,
    pub base_url: String,
    pub enabled: bool,
    pub last_synced_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdminSyncResponse {
    pub server_id: String,
    /// Id of the enqueued `beam_sync` job — poll `GET /admin/jobs` for progress.
    pub job_id: String,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdminJobResponse {
    pub id: String,
    pub job_type: String,
    pub status: String,
    pub attempts: u32,
    pub created_at: String,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdminJobListResponse {
    pub jobs: Vec<AdminJobResponse>,
}
