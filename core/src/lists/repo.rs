//! Thin SeaORM queries for lists and list items. Lists leave by setting
//! `deleted_at`; only an item's membership row is ever removed.

use sea_orm::ActiveValue::{Set, Unchanged};
use std::collections::HashMap;

use sea_orm::sea_query::Query;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, Order, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect,
};
use uuid::Uuid;

use crate::db::now;
use crate::entity::{entry_tag, library_entry, list, list_item, media};

use super::filter::SmartFilter;

pub async fn insert_list<C: ConnectionTrait>(
    db: &C,
    model: list::ActiveModel,
) -> Result<list::Model, DbErr> {
    model.insert(db).await
}

pub async fn find_list<C: ConnectionTrait>(db: &C, id: Uuid) -> Result<Option<list::Model>, DbErr> {
    list::Entity::find_by_id(id).one(db).await
}

/// A list, locked for the rest of the caller's transaction, so writes to
/// its items are serialised (two appends cannot pick the same position).
pub async fn lock_list<C: ConnectionTrait>(db: &C, id: Uuid) -> Result<Option<list::Model>, DbErr> {
    list::Entity::find_by_id(id).lock_exclusive().one(db).await
}

/// A user's live lists, oldest first.
pub async fn lists_for_user<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
) -> Result<Vec<list::Model>, DbErr> {
    list::Entity::find()
        .filter(list::Column::UserId.eq(user_id))
        .filter(list::Column::DeletedAt.is_null())
        .order_by(list::Column::CreatedAt, Order::Asc)
        .all(db)
        .await
}

/// A user's soft-deleted lists, most recently deleted first.
pub async fn trashed_lists<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
) -> Result<Vec<list::Model>, DbErr> {
    list::Entity::find()
        .filter(list::Column::UserId.eq(user_id))
        .filter(list::Column::DeletedAt.is_not_null())
        .order_by(list::Column::DeletedAt, Order::Desc)
        .all(db)
        .await
}

pub async fn update_list<C: ConnectionTrait>(
    db: &C,
    model: list::ActiveModel,
) -> Result<list::Model, DbErr> {
    model.update(db).await
}

pub async fn set_list_deleted<C: ConnectionTrait>(
    db: &C,
    id: Uuid,
    deleted: bool,
) -> Result<list::Model, DbErr> {
    list::ActiveModel {
        id: Unchanged(id),
        deleted_at: Set(deleted.then(now)),
        ..Default::default()
    }
    .update(db)
    .await
}

pub async fn insert_item<C: ConnectionTrait>(
    db: &C,
    model: list_item::ActiveModel,
) -> Result<list_item::Model, DbErr> {
    model.insert(db).await
}

pub async fn remove_item<C: ConnectionTrait>(
    db: &C,
    list_id: Uuid,
    media_id: Uuid,
) -> Result<bool, DbErr> {
    let result = list_item::Entity::delete_many()
        .filter(list_item::Column::ListId.eq(list_id))
        .filter(list_item::Column::MediaId.eq(media_id))
        .exec(db)
        .await?;
    Ok(result.rows_affected > 0)
}

/// A list's items in position order.
pub async fn items_for_list<C: ConnectionTrait>(
    db: &C,
    list_id: Uuid,
) -> Result<Vec<list_item::Model>, DbErr> {
    list_item::Entity::find()
        .filter(list_item::Column::ListId.eq(list_id))
        .order_by(list_item::Column::Position, Order::Asc)
        .all(db)
        .await
}

pub async fn count_items<C: ConnectionTrait>(db: &C, list_id: Uuid) -> Result<u64, DbErr> {
    list_item::Entity::find()
        .filter(list_item::Column::ListId.eq(list_id))
        .count(db)
        .await
}

/// How many items each of `list_ids` holds; lists with none are absent.
pub async fn count_items_many<C: ConnectionTrait>(
    db: &C,
    list_ids: &[Uuid],
) -> Result<HashMap<Uuid, u64>, DbErr> {
    if list_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(Uuid, i64)> = list_item::Entity::find()
        .select_only()
        .column(list_item::Column::ListId)
        .column_as(list_item::Column::Id.count(), "n")
        .filter(list_item::Column::ListId.is_in(list_ids.to_vec()))
        .group_by(list_item::Column::ListId)
        .into_tuple()
        .all(db)
        .await?;
    Ok(rows
        .into_iter()
        .map(|(id, n)| (id, u64::try_from(n).unwrap_or(0)))
        .collect())
}

pub async fn update_item<C: ConnectionTrait>(
    db: &C,
    model: list_item::ActiveModel,
) -> Result<list_item::Model, DbErr> {
    model.update(db).await
}

/// A smart list's library candidates: `user_id`'s live entries that pass the
/// predicates `filter` holds on entry and snapshot columns (kind, category,
/// status, own score, tag), the `limit` most recently changed, each with its
/// snapshot. The rest of the filter is applied by the caller.
pub async fn library_candidates<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    filter: &SmartFilter,
    limit: u64,
) -> Result<Vec<(library_entry::Model, media::Model)>, DbErr> {
    let mut titles = Query::select()
        .column(media::Column::Id)
        .from(media::Entity)
        .and_where(media::Column::Kind.is_in(filter.kinds.clone()))
        .to_owned();
    if let Some(categories) = filter.categories.as_ref().filter(|c| !c.is_empty()) {
        titles.and_where(media::Column::Category.is_in(categories.clone()));
    }
    let mut query = library_entry::Entity::find()
        .filter(library_entry::Column::UserId.eq(user_id))
        .filter(library_entry::Column::DeletedAt.is_null())
        .filter(library_entry::Column::MediaId.in_subquery(titles));
    if let Some(statuses) = filter.statuses.as_ref().filter(|s| !s.is_empty()) {
        query = query.filter(library_entry::Column::Status.is_in(statuses.clone()));
    }
    if let Some(lo) = filter.user_score_min {
        query = query.filter(library_entry::Column::Score.gte(lo));
    }
    if let Some(hi) = filter.user_score_max {
        query = query.filter(library_entry::Column::Score.lte(hi));
    }
    if let Some(tags) = filter.tags.as_ref().filter(|t| !t.is_empty()) {
        query = query.filter(
            library_entry::Column::Id.in_subquery(
                Query::select()
                    .column(entry_tag::Column::EntryId)
                    .from(entry_tag::Entity)
                    .and_where(entry_tag::Column::TagId.is_in(tags.clone()))
                    .to_owned(),
            ),
        );
    }
    let entries = query
        .order_by(library_entry::Column::UpdatedAt, Order::Desc)
        .order_by(library_entry::Column::Id, Order::Desc)
        .limit(limit)
        .all(db)
        .await?;
    crate::library::repo::with_media(db, entries).await
}
