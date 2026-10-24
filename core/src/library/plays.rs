//! The append-only `plays` table.
//!
//! A play is inserted once and afterwards can only be voided and restored:
//! this module has no other write, and the database refuses any other change
//! (see the baseline migration's `stacks_plays_void_only` trigger).

use sea_orm::ActiveValue::{Set, Unchanged};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, Order, QueryFilter,
    QueryOrder, QuerySelect,
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

/// A play, locked for the rest of the transaction, so a concurrent void or
/// restoration waits for this one and then sees its outcome.
pub async fn lock_play<C: ConnectionTrait>(
    db: &C,
    play_id: Uuid,
) -> Result<Option<play::Model>, DbErr> {
    play::Entity::find_by_id(play_id)
        .lock_exclusive()
        .one(db)
        .await
}

/// Mark a play voided. With [`restore_play`], the only write a play ever
/// receives after insert.
pub async fn void_play<C: ConnectionTrait>(db: &C, play_id: Uuid) -> Result<play::Model, DbErr> {
    play::ActiveModel {
        id: Unchanged(play_id),
        voided_at: Set(Some(now())),
        ..Default::default()
    }
    .update(db)
    .await
}

/// Make a voided play count again.
pub async fn restore_play<C: ConnectionTrait>(db: &C, play_id: Uuid) -> Result<play::Model, DbErr> {
    play::ActiveModel {
        id: Unchanged(play_id),
        voided_at: Set(None),
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

/// Insert many plays in one statement. The caller builds the full rows
/// (ids and timestamps included), so they are returned as stored.
pub async fn insert_plays<C: ConnectionTrait>(db: &C, rows: &[play::Model]) -> Result<(), DbErr> {
    for chunk in rows.chunks(crate::db::INSERT_CHUNK) {
        let models = chunk.iter().map(|p| play::ActiveModel {
            id: Set(p.id),
            run_id: Set(p.run_id),
            episode_id: Set(p.episode_id),
            watched_at: Set(p.watched_at),
            source: Set(p.source),
            import_id: Set(p.import_id),
            voided_at: Set(p.voided_at),
            created_at: Set(p.created_at),
            updated_at: Set(p.updated_at),
        });
        play::Entity::insert_many(models).exec(db).await?;
    }
    Ok(())
}

/// The live (not voided) plays of the given runs, oldest first.
pub async fn live_plays_in_runs<C: ConnectionTrait>(
    db: &C,
    run_ids: &[Uuid],
) -> Result<Vec<play::Model>, DbErr> {
    if run_ids.is_empty() {
        return Ok(Vec::new());
    }
    play::Entity::find()
        .filter(play::Column::RunId.is_in(run_ids.to_vec()))
        .filter(play::Column::VoidedAt.is_null())
        .order_by(play::Column::Id, Order::Asc)
        .all(db)
        .await
}
