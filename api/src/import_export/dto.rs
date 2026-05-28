use salvo::oapi::{ToParameters, ToSchema};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct StartImportRequest {
    /// Source format: `mal_xml`, `mal_json`, or `tmdb_json`.
    pub source: String,
    /// Conflict strategy: `skip` (default), `overwrite`, or `merge`.
    pub conflict_strategy: Option<String>,
    /// Base64-encoded export file bytes.
    pub data_base64: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ImportStatusResponse {
    pub job_id: String,
    pub status: String,
    pub attempts: u32,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, ToParameters)]
pub struct ExportQuery {
    /// `json` (default) or `csv`.
    pub format: Option<String>,
}
