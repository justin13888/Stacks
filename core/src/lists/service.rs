use chrono::Utc;
use sea_orm::{ActiveValue::Set, DatabaseConnection};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::entity::{user_list, user_list_item};

use super::{ListError, fractional_index, repo};

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
        if req.name.trim().is_empty() {
            return Err(ListError::EmptyName);
        }
        let now = Utc::now();
        let model = user_list::ActiveModel {
            id: Set(Ulid::new().to_string()),
            user_id: Set(user_id.to_string()),
            name: Set(req.name),
            description: Set(req.description),
            is_smart: Set(req.is_smart),
            filter_query: Set(req.filter_query),
            sort_order: Set(None),
            created_at: Set(now.into()),
            updated_at: Set(now.into()),
        };
        repo::create_list(&self.db, model).await
    }

    pub async fn get_list(
        &self,
        user_id: &str,
        list_id: &str,
    ) -> Result<user_list::Model, ListError> {
        let list = repo::find_list_by_id(&self.db, list_id)
            .await?
            .ok_or(ListError::NotFound)?;
        if list.user_id != user_id {
            return Err(ListError::NotOwner);
        }
        Ok(list)
    }

    pub async fn get_user_lists(&self, user_id: &str) -> Result<Vec<user_list::Model>, ListError> {
        repo::find_lists_for_user(&self.db, user_id).await
    }

    pub async fn update_list(
        &self,
        user_id: &str,
        list_id: &str,
        req: UpdateListRequest,
    ) -> Result<user_list::Model, ListError> {
        let list = self.get_list(user_id, list_id).await?;
        let mut active: user_list::ActiveModel = list.into();
        if let Some(name) = req.name {
            active.name = Set(name);
        }
        if let Some(desc) = req.description {
            active.description = Set(Some(desc));
        }
        if let Some(fq) = req.filter_query {
            active.filter_query = Set(Some(fq));
        }
        active.updated_at = Set(Utc::now().into());
        repo::update_list(&self.db, active).await
    }

    pub async fn delete_list(&self, user_id: &str, list_id: &str) -> Result<(), ListError> {
        self.get_list(user_id, list_id).await?;
        repo::delete_list(&self.db, list_id).await.map(|_| ())
    }

    pub async fn add_item(
        &self,
        user_id: &str,
        list_id: &str,
        cameo_media_id: &str,
        notes: Option<String>,
    ) -> Result<user_list_item::Model, ListError> {
        self.get_list(user_id, list_id).await?;

        // Compute position: append after the last item.
        let items = repo::find_items_for_list(&self.db, list_id).await?;
        let last_pos = items.last().map(|i| i.position);
        let position = fractional_index::midpoint(last_pos, None);

        let model = user_list_item::ActiveModel {
            list_id: Set(list_id.to_string()),
            cameo_media_id: Set(cameo_media_id.to_string()),
            position: Set(position),
            added_at: Set(Utc::now().into()),
            notes: Set(notes),
        };
        repo::add_item(&self.db, model).await
    }

    pub async fn remove_item(
        &self,
        user_id: &str,
        list_id: &str,
        cameo_media_id: &str,
    ) -> Result<(), ListError> {
        self.get_list(user_id, list_id).await?;
        let removed = repo::remove_item(&self.db, list_id, cameo_media_id).await?;
        if removed {
            Ok(())
        } else {
            Err(ListError::NotFound)
        }
    }

    pub async fn reorder_items(
        &self,
        user_id: &str,
        list_id: &str,
        reorder: ReorderItem,
    ) -> Result<user_list_item::Model, ListError> {
        self.get_list(user_id, list_id).await?;

        // Resolve before/after positions from the actual items.
        let items = repo::find_items_for_list(&self.db, list_id).await?;
        let before_pos = reorder.before.as_deref().and_then(|bid| {
            items
                .iter()
                .find(|i| i.cameo_media_id == bid)
                .map(|i| i.position)
        });
        let after_pos = reorder.after.as_deref().and_then(|aid| {
            items
                .iter()
                .find(|i| i.cameo_media_id == aid)
                .map(|i| i.position)
        });
        let position = fractional_index::midpoint(before_pos, after_pos);

        repo::update_item_position(&self.db, list_id, &reorder.cameo_media_id, position).await
    }

    /// Evaluate the filter_query of a smart list and return matching media IDs.
    pub async fn evaluate_smart_list(
        &self,
        _user_id: &str,
        _list_id: &str,
    ) -> Result<Vec<String>, ListError> {
        todo!("smart list evaluation requires cameo integration")
    }
}
