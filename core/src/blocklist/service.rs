use std::collections::HashSet;

use sea_orm::{DatabaseConnection, DbErr};
use uuid::Uuid;

use crate::entity::blocklist;
use crate::media::catalog;
use crate::media::id::MediaId;

use super::repo;

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

    pub async fn is_blocked(&self, user_id: Uuid, media_id: Uuid) -> Result<bool, DbErr> {
        repo::find_block(&self.db, user_id, media_id)
            .await
            .map(|b| b.is_some())
    }

    pub async fn list_blocked(&self, user_id: Uuid) -> Result<Vec<blocklist::Model>, DbErr> {
        repo::list_blocked(&self.db, user_id).await
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

    /// Keep only the public ids in `items` the user has not blocked.
    pub async fn filter_blocked(
        &self,
        user_id: Uuid,
        items: Vec<String>,
    ) -> Result<Vec<String>, DbErr> {
        let blocked = self.blocked_refs(user_id).await?;
        Ok(items
            .into_iter()
            .filter(|raw| MediaId::parse(raw).map_or(true, |id| !blocked.contains(&id)))
            .collect())
    }
}
