use salvo::oapi::{ToParameters, ToSchema};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, ToSchema, ToParameters)]
pub struct MediaSearchParams {
    pub q: String,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MediaSearchResponse {
    pub items: Vec<serde_json::Value>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MediaDetailResponse(pub serde_json::Value);

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SeasonsResponse {
    pub seasons: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct EpisodesResponse {
    pub episodes: Vec<serde_json::Value>,
}
