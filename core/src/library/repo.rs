//! Thin SeaORM queries for entries and runs (plays live in
//! [`super::plays`]).
//!
//! Transactions belong to the service; every function here takes any
//! [`ConnectionTrait`] so it runs inside one. No row is ever removed here:
//! entries leave the library by setting `deleted_at`.

use std::collections::HashMap;

use sea_orm::ActiveValue::{NotSet, Set, Unchanged};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, JoinType, Order,
    QueryFilter, QueryOrder, QuerySelect, RelationTrait,
};
use uuid::Uuid;

use crate::db::now;
use crate::entity::enums::{EntryStatus, MediaKind};
use crate::entity::{entry_progress, entry_tag, library_entry, media, tag, watch_run};

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
pub(crate) async fn with_media<C: ConnectionTrait>(
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

/// The tags each of `entry_ids` carries, by name; an entry with none is
/// absent. Two queries for the whole batch.
pub async fn tags_for_entries<C: ConnectionTrait>(
    db: &C,
    entry_ids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<tag::Model>>, DbErr> {
    if entry_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let assignments = entry_tag::Entity::find()
        .filter(entry_tag::Column::EntryId.is_in(entry_ids.to_vec()))
        .all(db)
        .await?;
    if assignments.is_empty() {
        return Ok(HashMap::new());
    }
    let tags: HashMap<Uuid, tag::Model> = tag::Entity::find()
        .filter(tag::Column::Id.is_in(assignments.iter().map(|a| a.tag_id)))
        .all(db)
        .await?
        .into_iter()
        .map(|t| (t.id, t))
        .collect();
    let mut by_entry: HashMap<Uuid, Vec<tag::Model>> = HashMap::new();
    for a in assignments {
        if let Some(t) = tags.get(&a.tag_id) {
            by_entry.entry(a.entry_id).or_default().push(t.clone());
        }
    }
    for carried in by_entry.values_mut() {
        carried.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    }
    Ok(by_entry)
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

/// A run of an entry by its number.
pub async fn run_by_number<C: ConnectionTrait>(
    db: &C,
    entry_id: Uuid,
    run_number: i32,
) -> Result<Option<watch_run::Model>, DbErr> {
    watch_run::Entity::find()
        .filter(watch_run::Column::EntryId.eq(entry_id))
        .filter(watch_run::Column::RunNumber.eq(run_number))
        .one(db)
        .await
}

/// The current (highest-numbered) run of each entry that has one.
pub async fn current_runs<C: ConnectionTrait>(
    db: &C,
    entry_ids: &[Uuid],
) -> Result<HashMap<Uuid, watch_run::Model>, DbErr> {
    if entry_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let runs = watch_run::Entity::find()
        .filter(watch_run::Column::EntryId.is_in(entry_ids.to_vec()))
        .order_by(watch_run::Column::RunNumber, Order::Asc)
        .all(db)
        .await?;
    // Ascending, so the last one inserted per entry is its highest.
    Ok(runs.into_iter().map(|r| (r.entry_id, r)).collect())
}

/// Entries by id, each with its snapshot, whatever their state.
pub async fn entries_by_ids<C: ConnectionTrait>(
    db: &C,
    entry_ids: &[Uuid],
) -> Result<Vec<(library_entry::Model, media::Model)>, DbErr> {
    if entry_ids.is_empty() {
        return Ok(Vec::new());
    }
    let entries = library_entry::Entity::find()
        .filter(library_entry::Column::Id.is_in(entry_ids.to_vec()))
        .all(db)
        .await?;
    with_media(db, entries).await
}

/// The user's live entries for shows they are watching, with snapshots.
pub async fn watching_shows<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
) -> Result<Vec<(library_entry::Model, media::Model)>, DbErr> {
    let entries = library_entry::Entity::find()
        .filter(library_entry::Column::UserId.eq(user_id))
        .filter(library_entry::Column::DeletedAt.is_null())
        .filter(library_entry::Column::Status.eq(EntryStatus::Watching))
        .join(JoinType::InnerJoin, library_entry::Relation::Media.def())
        .filter(media::Column::Kind.eq(MediaKind::Show))
        .all(db)
        .await?;
    with_media(db, entries).await
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
