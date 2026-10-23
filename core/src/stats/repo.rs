use sea_orm::{ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter};
use thiserror::Error;
use uuid::Uuid;

use crate::entity::{episode, library_entry, media, play, watch_run};

#[derive(Debug, Error)]
pub enum StatsError {
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}

/// Everything the stats are computed from, loaded in five queries.
pub struct StatsData {
    pub entries: Vec<(library_entry::Model, media::Model)>,
    pub runs: Vec<watch_run::Model>,
    /// Non-voided plays only.
    pub plays: Vec<play::Model>,
    pub episodes: Vec<episode::Model>,
}

pub async fn load<C: ConnectionTrait>(db: &C, user_id: Uuid) -> Result<StatsData, DbErr> {
    let entries = crate::library::repo::all_entries(db, user_id).await?;
    let entry_ids: Vec<Uuid> = entries.iter().map(|(e, _)| e.id).collect();
    let runs = if entry_ids.is_empty() {
        Vec::new()
    } else {
        watch_run::Entity::find()
            .filter(watch_run::Column::EntryId.is_in(entry_ids))
            .all(db)
            .await?
    };
    let run_ids: Vec<Uuid> = runs.iter().map(|r| r.id).collect();
    let plays = if run_ids.is_empty() {
        Vec::new()
    } else {
        play::Entity::find()
            .filter(play::Column::RunId.is_in(run_ids))
            .filter(play::Column::VoidedAt.is_null())
            .all(db)
            .await?
    };
    let episode_ids: Vec<Uuid> = plays.iter().filter_map(|p| p.episode_id).collect();
    let episodes = if episode_ids.is_empty() {
        Vec::new()
    } else {
        episode::Entity::find()
            .filter(episode::Column::Id.is_in(episode_ids))
            .all(db)
            .await?
    };
    Ok(StatsData {
        entries,
        runs,
        plays,
        episodes,
    })
}
