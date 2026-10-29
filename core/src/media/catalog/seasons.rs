//! Writing a title's seasons and episodes into the snapshot. Every write
//! only adds or fills in: a field the provider left empty keeps its stored
//! value.

use std::collections::{BTreeMap, HashSet};

use sea_orm::sea_query::{Expr, OnConflict};
use sea_orm::{
    ActiveValue::NotSet, ActiveValue::Set, ColumnTrait, ConnectionTrait, DbErr, EntityTrait,
    QueryFilter, TransactionSession, TransactionTrait,
};
use uuid::Uuid;

use crate::entity::{episode, season};
use crate::media::mapping::parse_date;
use crate::media::models::{EpisodeInfo, SeasonInfo};

/// Episode rows per insert statement: 10 bound columns each keeps a
/// statement far under PostgreSQL's 65,535 bind parameters.
pub const EPISODE_BATCH: usize = 1_000;

/// Store season summaries for a title. A field the provider left empty keeps
/// its stored value, so a sparse refresh never erases what is known.
pub async fn upsert_seasons<C: ConnectionTrait>(
    db: &C,
    media_id: Uuid,
    seasons: &[SeasonInfo],
) -> Result<(), DbErr> {
    for s in seasons {
        upsert_season(db, media_id, s).await?;
    }
    Ok(())
}

async fn upsert_season<C: ConnectionTrait>(
    db: &C,
    media_id: Uuid,
    s: &SeasonInfo,
) -> Result<season::Model, DbErr> {
    let number = i32::try_from(s.season_number)
        .map_err(|_| DbErr::Custom(format!("season number {} out of range", s.season_number)))?;
    let keep = |col: &str| Expr::cust(format!("COALESCE(excluded.{col}, seasons.{col})"));
    season::Entity::insert(season::ActiveModel {
        id: Set(Uuid::now_v7()),
        media_id: Set(media_id),
        number: Set(number),
        title: Set(s.name.clone()),
        overview: Set(s.overview.clone()),
        air_date: Set(parse_date(s.air_date.as_deref())),
        poster_url: Set(s.poster_url.clone()),
        episode_count: Set(s.episode_count.and_then(|c| i32::try_from(c).ok())),
        // Detection state, not catalog data: an upsert never touches it.
        announced_through: Set(None),
        created_at: NotSet,
        updated_at: NotSet,
    })
    .on_conflict(
        OnConflict::columns([season::Column::MediaId, season::Column::Number])
            .value(season::Column::Title, keep("title"))
            .value(season::Column::Overview, keep("overview"))
            .value(season::Column::AirDate, keep("air_date"))
            .value(season::Column::PosterUrl, keep("poster_url"))
            .value(season::Column::EpisodeCount, keep("episode_count"))
            .to_owned(),
    )
    .exec(db)
    .await?;
    season::Entity::find()
        .filter(season::Column::MediaId.eq(media_id))
        .filter(season::Column::Number.eq(number))
        .one(db)
        .await?
        .ok_or_else(|| DbErr::RecordNotFound(format!("season {number}")))
}

/// Store one season and its episodes in one transaction, returning the
/// episode rows that did not exist before. Like [`upsert_seasons`], it only
/// ever adds or fills in. Episodes go in [`EPISODE_BATCH`]-row statements, so
/// a long show costs a few statements rather than one per episode; an
/// episode number listed twice keeps its last listing.
pub async fn upsert_season_episodes<C>(
    db: &C,
    media_id: Uuid,
    season_info: &SeasonInfo,
    episodes: &[EpisodeInfo],
) -> Result<Vec<episode::Model>, DbErr>
where
    C: ConnectionTrait + TransactionTrait,
{
    let txn = db.begin().await?;
    let season = upsert_season(&txn, media_id, season_info).await?;
    let stored = |txn| {
        episode::Entity::find()
            .filter(episode::Column::SeasonId.eq(season.id))
            .all(txn)
    };
    let existing: HashSet<i32> = stored(&txn).await?.into_iter().map(|e| e.number).collect();
    // One row per number: one statement may not upsert a row twice.
    let rows: BTreeMap<i32, episode::ActiveModel> = episodes
        .iter()
        .filter_map(|e| Some((i32::try_from(e.episode_number).ok()?, e)))
        .map(|(number, e)| {
            let row = episode::ActiveModel {
                id: Set(Uuid::now_v7()),
                media_id: Set(media_id),
                season_id: Set(season.id),
                number: Set(number),
                absolute_number: Set(e
                    .absolute_number
                    .and_then(|n| i32::try_from(n).ok())
                    .filter(|n| *n >= 1)),
                title: Set(e.name.clone()),
                overview: Set(e.overview.clone()),
                air_date: Set(parse_date(e.air_date.as_deref())),
                runtime_minutes: Set(e.runtime_minutes.and_then(|r| i32::try_from(r).ok())),
                still_url: Set(e.still_url.clone()),
                created_at: NotSet,
                updated_at: NotSet,
            };
            (number, row)
        })
        .collect();
    let rows: Vec<episode::ActiveModel> = rows.into_values().collect();
    let keep = |col: &str| Expr::cust(format!("COALESCE(excluded.{col}, episodes.{col})"));
    for batch in rows.chunks(EPISODE_BATCH) {
        episode::Entity::insert_many(batch.iter().cloned())
            .on_conflict(
                OnConflict::columns([episode::Column::SeasonId, episode::Column::Number])
                    .value(episode::Column::AbsoluteNumber, keep("absolute_number"))
                    .value(episode::Column::Title, keep("title"))
                    .value(episode::Column::Overview, keep("overview"))
                    .value(episode::Column::AirDate, keep("air_date"))
                    .value(episode::Column::RuntimeMinutes, keep("runtime_minutes"))
                    .value(episode::Column::StillUrl, keep("still_url"))
                    .to_owned(),
            )
            .exec_without_returning(&txn)
            .await?;
    }
    let added = stored(&txn).await?;
    txn.commit().await?;
    Ok(added
        .into_iter()
        .filter(|e| !existing.contains(&e.number))
        .collect())
}
