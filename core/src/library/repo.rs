//! Thin SeaORM queries for entries and runs (plays live in
//! [`super::plays`]).
//!
//! Transactions belong to the service; every function here takes any
//! [`ConnectionTrait`] so it runs inside one. No row is ever removed here:
//! entries leave the library by setting `deleted_at`.

use sea_orm::ActiveValue::{NotSet, Set, Unchanged};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, JoinType, Order,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, RelationTrait,
};
use uuid::Uuid;

use crate::db::now;
use crate::entity::enums::{EntryStatus, MediaKind};
use crate::entity::{entry_progress, library_entry, media, watch_run};

/// Filters accepted by [`list_entries`].
#[derive(Debug, Default, Clone)]
pub struct LibraryFilter {
    /// Restrict to one status.
    pub status: Option<EntryStatus>,
    /// Restrict to movies or shows.
    pub kind: Option<MediaKind>,
    /// Restrict to favourites.
    pub favourites_only: bool,
}

/// The user's entry for a title, including a soft-deleted one.
pub async fn find_entry<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    media_id: Uuid,
) -> Result<Option<library_entry::Model>, DbErr> {
    library_entry::Entity::find()
        .filter(library_entry::Column::UserId.eq(user_id))
        .filter(library_entry::Column::MediaId.eq(media_id))
        .one(db)
        .await
}

/// As [`find_entry`], but row-locked for the rest of the transaction so two
/// concurrent mutations of one entry serialise instead of interleaving.
pub async fn lock_entry<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    media_id: Uuid,
) -> Result<Option<library_entry::Model>, DbErr> {
    library_entry::Entity::find()
        .filter(library_entry::Column::UserId.eq(user_id))
        .filter(library_entry::Column::MediaId.eq(media_id))
        .lock_exclusive()
        .one(db)
        .await
}

pub async fn insert_entry<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    media_id: Uuid,
    status: EntryStatus,
) -> Result<library_entry::Model, DbErr> {
    library_entry::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user_id),
        media_id: Set(media_id),
        status: Set(status),
        score: Set(None),
        favourite: Set(false),
        notes: Set(None),
        deleted_at: Set(None),
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(db)
    .await
}

/// Write a partial update built by [`super::EntryPatch::to_update`].
pub async fn update_entry<C: ConnectionTrait>(
    db: &C,
    am: library_entry::ActiveModel,
) -> Result<library_entry::Model, DbErr> {
    am.update(db).await
}

/// Set or clear `deleted_at` — the only way an entry leaves the library.
pub async fn set_deleted<C: ConnectionTrait>(
    db: &C,
    entry_id: Uuid,
    deleted: bool,
) -> Result<library_entry::Model, DbErr> {
    library_entry::ActiveModel {
        id: Unchanged(entry_id),
        deleted_at: Set(deleted.then(now)),
        ..Default::default()
    }
    .update(db)
    .await
}

/// One page of the user's live entries with their snapshots, most recently
/// updated first. Returns the rows and the total page count.
pub async fn list_entries<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    filter: &LibraryFilter,
    page: u32,
    per_page: u64,
) -> Result<(Vec<(library_entry::Model, media::Model)>, u32), DbErr> {
    let mut query = library_entry::Entity::find()
        .filter(library_entry::Column::UserId.eq(user_id))
        .filter(library_entry::Column::DeletedAt.is_null());
    if let Some(status) = filter.status {
        query = query.filter(library_entry::Column::Status.eq(status));
    }
    if filter.favourites_only {
        query = query.filter(library_entry::Column::Favourite.eq(true));
    }
    if let Some(kind) = filter.kind {
        query = query
            .join(JoinType::InnerJoin, library_entry::Relation::Media.def())
            .filter(media::Column::Kind.eq(kind));
    }
    let paginator = query
        .order_by(library_entry::Column::UpdatedAt, Order::Desc)
        .order_by(library_entry::Column::Id, Order::Desc)
        .paginate(db, per_page.max(1));
    let total_pages = paginator.num_pages().await? as u32;
    // `paginate` is 0-indexed; cursors carry 1-indexed page numbers.
    let entries = paginator.fetch_page(page.saturating_sub(1) as u64).await?;
    with_media(db, entries)
        .await
        .map(|rows| (rows, total_pages))
}

/// The user's soft-deleted entries, most recently deleted first.
pub async fn list_trash<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
) -> Result<Vec<(library_entry::Model, media::Model)>, DbErr> {
    let entries = library_entry::Entity::find()
        .filter(library_entry::Column::UserId.eq(user_id))
        .filter(library_entry::Column::DeletedAt.is_not_null())
        .order_by(library_entry::Column::DeletedAt, Order::Desc)
        .all(db)
        .await?;
    with_media(db, entries).await
}

/// Pair each entry with its snapshot row (one query for the whole batch).
async fn with_media<C: ConnectionTrait>(
    db: &C,
    entries: Vec<library_entry::Model>,
) -> Result<Vec<(library_entry::Model, media::Model)>, DbErr> {
    let ids: Vec<Uuid> = entries.iter().map(|e| e.media_id).collect();
    let media: std::collections::HashMap<Uuid, media::Model> = media::Entity::find()
        .filter(media::Column::Id.is_in(ids))
        .all(db)
        .await?
        .into_iter()
        .map(|m| (m.id, m))
        .collect();
    Ok(entries
        .into_iter()
        .filter_map(|e| {
            let m = media.get(&e.media_id)?.clone();
            Some((e, m))
        })
        .collect())
}

/// Every live entry of a user (for stats and export).
pub async fn all_entries<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
) -> Result<Vec<(library_entry::Model, media::Model)>, DbErr> {
    let entries = library_entry::Entity::find()
        .filter(library_entry::Column::UserId.eq(user_id))
        .filter(library_entry::Column::DeletedAt.is_null())
        .order_by(library_entry::Column::UpdatedAt, Order::Desc)
        .all(db)
        .await?;
    with_media(db, entries).await
}

pub async fn progress<C: ConnectionTrait>(
    db: &C,
    entry_id: Uuid,
) -> Result<Option<entry_progress::Model>, DbErr> {
    entry_progress::Entity::find_by_id(entry_id).one(db).await
}

pub async fn progress_many<C: ConnectionTrait>(
    db: &C,
    entry_ids: Vec<Uuid>,
) -> Result<Vec<entry_progress::Model>, DbErr> {
    if entry_ids.is_empty() {
        return Ok(Vec::new());
    }
    entry_progress::Entity::find()
        .filter(entry_progress::Column::EntryId.is_in(entry_ids))
        .all(db)
        .await
}

/// The highest-numbered run of an entry — the one plays are logged against.
pub async fn current_run<C: ConnectionTrait>(
    db: &C,
    entry_id: Uuid,
) -> Result<Option<watch_run::Model>, DbErr> {
    watch_run::Entity::find()
        .filter(watch_run::Column::EntryId.eq(entry_id))
        .order_by(watch_run::Column::RunNumber, Order::Desc)
        .one(db)
        .await
}

pub async fn runs_for_entry<C: ConnectionTrait>(
    db: &C,
    entry_id: Uuid,
) -> Result<Vec<watch_run::Model>, DbErr> {
    watch_run::Entity::find()
        .filter(watch_run::Column::EntryId.eq(entry_id))
        .order_by(watch_run::Column::RunNumber, Order::Asc)
        .all(db)
        .await
}

pub async fn find_run<C: ConnectionTrait>(
    db: &C,
    run_id: Uuid,
) -> Result<Option<watch_run::Model>, DbErr> {
    watch_run::Entity::find_by_id(run_id).one(db).await
}

pub async fn insert_run<C: ConnectionTrait>(
    db: &C,
    am: watch_run::ActiveModel,
) -> Result<watch_run::Model, DbErr> {
    am.insert(db).await
}

pub async fn update_run<C: ConnectionTrait>(
    db: &C,
    am: watch_run::ActiveModel,
) -> Result<watch_run::Model, DbErr> {
    am.update(db).await
}
