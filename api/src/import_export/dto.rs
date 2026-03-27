use salvo::oapi::{ToParameters, ToSchema};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ImportStatusResponse {
    pub job_id: String,
    pub status: String,
    pub progress: Option<u32>,
    pub report: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, ToParameters)]
pub struct ExportQuery {
    pub format: String,
}
