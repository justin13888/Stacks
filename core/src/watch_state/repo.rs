use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    sea_query::OnConflict,
};

use crate::entity::{episode_watch_status, watch_status};

use super::WatchStateError;

pub async fn upsert_status(
    db: &DatabaseConnection,
    model: watch_status::ActiveModel,
) -> Result<watch_status::Model, WatchStateError> {
    watch_status::Entity::insert(model)
        .on_conflict(
            OnConflict::columns([
                watch_status::Column::UserId,
                watch_status::Column::CameoMediaId,
            ])
            .update_columns([
                watch_status::Column::Status,
                watch_status::Column::Score,
                watch_status::Column::ScoreDate,
                watch_status::Column::StartedAt,
                watch_status::Column::CompletedAt,
                watch_status::Column::RepeatCount,
                watch_status::Column::Notes,
                watch_status::Column::IsFavorite,
                watch_status::Column::UpdatedAt,
            ])
            .to_owned(),
        )
        .exec_with_returning(db)
        .await
        .map_err(WatchStateError::Db)
}

pub async fn find_status(
    db: &DatabaseConnection,
    user_id: &str,
    cameo_media_id: &str,
) -> Result<Option<watch_status::Model>, WatchStateError> {
    watch_status::Entity::find()
        .filter(watch_status::Column::UserId.eq(user_id))
        .filter(watch_status::Column::CameoMediaId.eq(cameo_media_id))
        .one(db)
        .await
        .map_err(WatchStateError::Db)
}

pub async fn delete_status(
    db: &DatabaseConnection,
    user_id: &str,
    cameo_media_id: &str,
) -> Result<bool, WatchStateError> {
    let result = watch_status::Entity::delete_many()
        .filter(watch_status::Column::UserId.eq(user_id))
        .filter(watch_status::Column::CameoMediaId.eq(cameo_media_id))
        .exec(db)
        .await
        .map_err(WatchStateError::Db)?;
    Ok(result.rows_affected > 0)
}

pub async fn upsert_episode_status(
    db: &DatabaseConnection,
    model: episode_watch_status::ActiveModel,
) -> Result<episode_watch_status::Model, WatchStateError> {
    episode_watch_status::Entity::insert(model)
        .on_conflict(
            OnConflict::columns([
                episode_watch_status::Column::UserId,
                episode_watch_status::Column::CameoEpisodeId,
            ])
            .update_columns([
                episode_watch_status::Column::CameoMediaId,
                episode_watch_status::Column::Watched,
                episode_watch_status::Column::WatchedAt,
                episode_watch_status::Column::ProgressSeconds,
                episode_watch_status::Column::UpdatedAt,
            ])
            .to_owned(),
        )
        .exec_with_returning(db)
        .await
        .map_err(WatchStateError::Db)
}

pub async fn find_episode_status(
    db: &DatabaseConnection,
    user_id: &str,
    cameo_episode_id: &str,
) -> Result<Option<episode_watch_status::Model>, WatchStateError> {
    episode_watch_status::Entity::find()
        .filter(episode_watch_status::Column::UserId.eq(user_id))
        .filter(episode_watch_status::Column::CameoEpisodeId.eq(cameo_episode_id))
        .one(db)
        .await
        .map_err(WatchStateError::Db)
}

pub async fn find_episodes_for_media(
    db: &DatabaseConnection,
    user_id: &str,
    cameo_media_id: &str,
) -> Result<Vec<episode_watch_status::Model>, WatchStateError> {
    episode_watch_status::Entity::find()
        .filter(episode_watch_status::Column::UserId.eq(user_id))
        .filter(episode_watch_status::Column::CameoMediaId.eq(cameo_media_id))
        .all(db)
        .await
        .map_err(WatchStateError::Db)
}

/// Filters accepted by [`list_statuses`].
#[derive(Debug, Default, Clone)]
pub struct LibraryFilter {
    /// Restrict to a single watch status (e.g. `watching`).
    pub status: Option<String>,
    /// Restrict to `movie` or `tv`.
    pub media_type: Option<String>,
    /// Restrict to favourites only.
    pub favorites_only: bool,
}

/// One page of the user's tracked media, most recently updated first.
///
/// Returns the rows plus the total page count so the caller can build the next
/// cursor with [`crate::pagination::next_page_cursor`].
pub async fn list_statuses(
    db: &DatabaseConnection,
    user_id: &str,
    filter: &LibraryFilter,
    page: u32,
    per_page: u64,
) -> Result<(Vec<watch_status::Model>, u32), WatchStateError> {
    let mut query = watch_status::Entity::find().filter(watch_status::Column::UserId.eq(user_id));

    if let Some(status) = &filter.status {
        query = query.filter(watch_status::Column::Status.eq(status.as_str()));
    }
    if let Some(media_type) = &filter.media_type {
        // Canonical ids are `<provider>:<kind>:<numeric>`, so matching the
        // delimited kind segment avoids false hits on provider or numeric parts.
        query =
            query.filter(watch_status::Column::CameoMediaId.contains(format!(":{media_type}:")));
    }
    if filter.favorites_only {
        query = query.filter(watch_status::Column::IsFavorite.eq(true));
    }

    let paginator = query
        .order_by_desc(watch_status::Column::UpdatedAt)
        .paginate(db, per_page);

    let total_pages = paginator.num_pages().await? as u32;
    // `paginate` is 0-indexed; our cursors are 1-indexed page numbers.
    let items = paginator.fetch_page(page.saturating_sub(1) as u64).await?;

    Ok((items, total_pages))
}
