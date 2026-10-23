//! The append-only `plays` table.
//!
//! A play is inserted once and afterwards can only be voided: this module has
//! no other write, and the database refuses any other change (see the
//! baseline migration's `stacks_plays_void_only` trigger).

use sea_orm::ActiveValue::{Set, Unchanged};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, Order, QueryFilter,
    QueryOrder,
};
use uuid::Uuid;

use crate::db::now;
use crate::entity::play;

use super::repo::runs_for_entry;

pub async fn insert_play<C: ConnectionTrait>(
    db: &C,
    am: play::ActiveModel,
) -> Result<play::Model, DbErr> {
    am.insert(db).await
}

pub async fn find_play<C: ConnectionTrait>(
    db: &C,
    play_id: Uuid,
) -> Result<Option<play::Model>, DbErr> {
    play::Entity::find_by_id(play_id).one(db).await
}

/// Mark a play voided. The only write a play ever receives after insert.
pub async fn void_play<C: ConnectionTrait>(db: &C, play_id: Uuid) -> Result<play::Model, DbErr> {
    play::ActiveModel {
        id: Unchanged(play_id),
        voided_at: Set(Some(now())),
        ..Default::default()
    }
    .update(db)
    .await
}

/// The latest non-voided play of an episode within a run.
pub async fn latest_active_episode_play<C: ConnectionTrait>(
    db: &C,
    run_id: Uuid,
    episode_id: Uuid,
) -> Result<Option<play::Model>, DbErr> {
    play::Entity::find()
        .filter(play::Column::RunId.eq(run_id))
        .filter(play::Column::EpisodeId.eq(episode_id))
        .filter(play::Column::VoidedAt.is_null())
        .order_by(play::Column::Id, Order::Desc)
        .one(db)
        .await
}

/// Every play of every run of an entry, oldest first (voided ones included).
pub async fn plays_for_entry<C: ConnectionTrait>(
    db: &C,
    entry_id: Uuid,
) -> Result<Vec<play::Model>, DbErr> {
    let run_ids: Vec<Uuid> = runs_for_entry(db, entry_id)
        .await?
        .into_iter()
        .map(|r| r.id)
        .collect();
    if run_ids.is_empty() {
        return Ok(Vec::new());
    }
    play::Entity::find()
        .filter(play::Column::RunId.is_in(run_ids))
        .order_by(play::Column::Id, Order::Asc)
        .all(db)
        .await
}
