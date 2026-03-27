use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdminCreateServerRequest {
    pub name: String,
    pub server_type: String,
    pub base_url: String,
    pub api_key: Option<String>,
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
