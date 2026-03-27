use sea_orm::DatabaseConnection;

use crate::entity::blocklist;

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
        let _ = (user_id, cameo_media_id);
        todo!()
    }

    pub async fn unblock(&self, user_id: &str, cameo_media_id: &str) -> Result<(), sea_orm::DbErr> {
        let _ = (user_id, cameo_media_id);
        todo!()
    }

    pub async fn is_blocked(
        &self,
        user_id: &str,
        cameo_media_id: &str,
    ) -> Result<bool, sea_orm::DbErr> {
        let _ = (user_id, cameo_media_id);
        todo!()
    }

    pub async fn list_blocked(
        &self,
        user_id: &str,
    ) -> Result<Vec<blocklist::Model>, sea_orm::DbErr> {
        let _ = user_id;
        todo!()
    }

    /// Returns only the IDs from `items` that are NOT blocked by the user.
    pub async fn filter_blocked(
        &self,
        user_id: &str,
        items: Vec<String>,
    ) -> Result<Vec<String>, sea_orm::DbErr> {
        let _ = (user_id, items);
        todo!()
    }
}
