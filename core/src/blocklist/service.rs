use chrono::Utc;
use sea_orm::{ActiveValue::Set, DatabaseConnection};
use std::collections::HashSet;

use crate::entity::blocklist;

use super::repo;

pub struct BlocklistService {
    pub db: DatabaseConnection,
}

impl BlocklistService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn block(
        &self,
        user_id: &str,
        cameo_media_id: &str,
    ) -> Result<blocklist::Model, sea_orm::DbErr> {
        let model = blocklist::ActiveModel {
            user_id: Set(user_id.to_string()),
            cameo_media_id: Set(cameo_media_id.to_string()),
            added_at: Set(Utc::now().into()),
        };
        // Upsert: if already blocked, return existing.
        match repo::insert_block(&self.db, model).await {
            Ok(m) => Ok(m),
            Err(e) if e.to_string().contains("duplicate") || e.to_string().contains("unique") => {
                repo::find_block(&self.db, user_id, cameo_media_id)
                    .await?
                    .ok_or(e)
            }
            Err(e) => Err(e),
        }
    }

    pub async fn unblock(&self, user_id: &str, cameo_media_id: &str) -> Result<(), sea_orm::DbErr> {
        repo::delete_block(&self.db, user_id, cameo_media_id)
            .await
            .map(|_| ())
    }

    pub async fn is_blocked(
        &self,
        user_id: &str,
        cameo_media_id: &str,
    ) -> Result<bool, sea_orm::DbErr> {
        repo::find_block(&self.db, user_id, cameo_media_id)
            .await
            .map(|opt| opt.is_some())
    }

    pub async fn list_blocked(
        &self,
        user_id: &str,
    ) -> Result<Vec<blocklist::Model>, sea_orm::DbErr> {
        repo::list_blocked(&self.db, user_id).await
    }

    /// Returns only the IDs from `items` that are NOT blocked by the user.
    pub async fn filter_blocked(
        &self,
        user_id: &str,
        items: Vec<String>,
    ) -> Result<Vec<String>, sea_orm::DbErr> {
        let blocked: HashSet<String> = self
            .list_blocked(user_id)
            .await?
            .into_iter()
            .map(|b| b.cameo_media_id)
            .collect();
        Ok(items
            .into_iter()
            .filter(|id| !blocked.contains(id))
            .collect())
    }
}
