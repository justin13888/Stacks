use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateListRequest {
    pub name: String,
    pub description: Option<String>,
    pub is_smart: Option<bool>,
    pub filter_query: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateListRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    /// Smart-list filter query. Only meaningful for smart lists.
    pub filter_query: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub is_smart: bool,
    pub item_count: u32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AddItemRequest {
    pub cameo_media_id: String,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ReorderItem {
    pub cameo_media_id: String,
    pub position: f64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ReorderRequest {
    pub items: Vec<ReorderItem>,
}
