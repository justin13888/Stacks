use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use crate::entity::{user_list, user_list_item};

use super::ListError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateListRequest {
    pub name: String,
    pub description: Option<String>,
    pub is_smart: bool,
    pub filter_query: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateListRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub filter_query: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorderItem {
    pub cameo_media_id: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

pub struct ListService {
    pub db: DatabaseConnection,
}

impl ListService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn create_list(
        &self,
        user_id: &str,
        req: CreateListRequest,
    ) -> Result<user_list::Model, ListError> {
        let _ = (user_id, req);
        todo!()
    }

    pub async fn get_list(
        &self,
        user_id: &str,
        list_id: &str,
    ) -> Result<user_list::Model, ListError> {
        let _ = (user_id, list_id);
        todo!()
    }

    pub async fn get_user_lists(&self, user_id: &str) -> Result<Vec<user_list::Model>, ListError> {
        let _ = user_id;
        todo!()
    }

    pub async fn update_list(
        &self,
        user_id: &str,
        list_id: &str,
        req: UpdateListRequest,
    ) -> Result<user_list::Model, ListError> {
        let _ = (user_id, list_id, req);
        todo!()
    }

    pub async fn delete_list(&self, user_id: &str, list_id: &str) -> Result<(), ListError> {
        let _ = (user_id, list_id);
        todo!()
    }

    pub async fn add_item(
        &self,
        user_id: &str,
        list_id: &str,
        cameo_media_id: &str,
        notes: Option<String>,
    ) -> Result<user_list_item::Model, ListError> {
        let _ = (user_id, list_id, cameo_media_id, notes);
        todo!()
    }

    pub async fn remove_item(
        &self,
        user_id: &str,
        list_id: &str,
        cameo_media_id: &str,
    ) -> Result<(), ListError> {
        let _ = (user_id, list_id, cameo_media_id);
        todo!()
    }

    pub async fn reorder_items(
        &self,
        user_id: &str,
        list_id: &str,
        reorder: ReorderItem,
    ) -> Result<user_list_item::Model, ListError> {
        let _ = (user_id, list_id, reorder);
        todo!()
    }

    /// Evaluate the filter_query of a smart list and return matching media IDs.
    pub async fn evaluate_smart_list(
        &self,
        user_id: &str,
        list_id: &str,
    ) -> Result<Vec<String>, ListError> {
        let _ = (user_id, list_id);
        todo!()
    }
}
