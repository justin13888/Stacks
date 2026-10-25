//! Blocklist reads only tests use (the `test-support` feature).

use super::*;

impl BlocklistService {
    pub async fn is_blocked(&self, user_id: Uuid, media_id: Uuid) -> Result<bool, DbErr> {
        repo::find_block(&self.db, user_id, media_id)
            .await
            .map(|b| b.is_some())
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
