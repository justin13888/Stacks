use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BlocklistEntryResponse {
    pub cameo_media_id: String,
    pub added_at: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AddBlocklistRequest {
    pub cameo_media_id: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BlocklistResponse {
    pub items: Vec<BlocklistEntryResponse>,
}
