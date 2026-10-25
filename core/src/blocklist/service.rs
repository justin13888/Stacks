use std::collections::HashSet;

use sea_orm::{DatabaseConnection, DbErr};
use uuid::Uuid;

use crate::entity::{blocklist, media};
use crate::media::catalog;
use crate::media::id::MediaId;

use super::repo;

#[cfg(feature = "test-support")]
mod test_support;

/// Titles a user never wants suggested.
pub struct BlocklistService {
    db: DatabaseConnection,
}

impl BlocklistService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Block a title (idempotent).
    pub async fn block(&self, user_id: Uuid, media_id: Uuid) -> Result<blocklist::Model, DbErr> {
        repo::insert_block(&self.db, user_id, media_id).await?;
        repo::find_block(&self.db, user_id, media_id)
            .await?
            .ok_or_else(|| DbErr::RecordNotFound(media_id.to_string()))
    }

    /// Unblock a title. Unblocking one that is not blocked is a no-op.
    pub async fn unblock(&self, user_id: Uuid, media_id: Uuid) -> Result<(), DbErr> {
        repo::remove_block(&self.db, user_id, media_id)
            .await
            .map(|_| ())
    }

    pub async fn list_blocked(&self, user_id: Uuid) -> Result<Vec<blocklist::Model>, DbErr> {
        repo::list_blocked(&self.db, user_id).await
    }

    /// The user's blocked titles, most recent first, each with its catalog
    /// row.
    pub async fn list_blocked_media(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<(blocklist::Model, media::Model)>, DbErr> {
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
        let blocked = self.list_blocked(user_id).await?;
        if blocked.is_empty() {
            return Ok(Vec::new());
        }
        let mut rows: std::collections::HashMap<Uuid, media::Model> = media::Entity::find()
            .filter(media::Column::Id.is_in(blocked.iter().map(|b| b.media_id)))
            .all(&self.db)
            .await?
            .into_iter()
            .map(|m| (m.id, m))
            .collect();
        // `blocklist.media_id` references `media`, so every row is there.
        Ok(blocked
            .into_iter()
            .filter_map(|b| {
                let m = rows.remove(&b.media_id)?;
                Some((b, m))
            })
            .collect())
    }

    /// The public ids of the user's blocked titles.
    pub async fn blocked_refs(&self, user_id: Uuid) -> Result<HashSet<MediaId>, DbErr> {
        let blocked = self.list_blocked(user_id).await?;
        let ids: Vec<Uuid> = blocked.iter().map(|b| b.media_id).collect();
        Ok(catalog::primary_refs(&self.db, &ids)
            .await?
            .into_values()
            .collect())
    }
}
