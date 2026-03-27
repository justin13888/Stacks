use salvo::oapi::{ToParameters, ToSchema};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, ToSchema, ToParameters)]
pub struct DiscoverQuery {
    pub media_type: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DiscoverResponse {
    pub items: Vec<serde_json::Value>,
    pub next_cursor: Option<String>,
}
