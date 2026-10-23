//! The local catalog snapshot: `media`, `media_refs`, `seasons`, `episodes`.
//!
//! Every tracked title has a snapshot row, so the library renders from the
//! database and survives a provider outage. This module is the minimal upsert
//! path; refreshing stale snapshots belongs to the metadata layer.
//!
//! Functions take any [`ConnectionTrait`], so a caller can run them inside its
//! own transaction.

use std::collections::HashMap;

use sea_orm::sea_query::{Expr, OnConflict};
use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ActiveValue::Set, ColumnTrait, Condition,
    ConnectionTrait, DbErr, EntityTrait, QueryFilter, QueryOrder, TransactionSession,
    TransactionTrait,
};
use uuid::Uuid;

use crate::db::now;
use crate::entity::enums::{MediaFormat, RefProvider, ReleaseStatus};
use crate::entity::{episode, media, media_ref, season};

use super::category::{CategoryInput, derive_category};
use super::id::{MediaId, MediaKind, Provider};
use super::models::{EpisodeInfo, SeasonInfo};

/// Everything stored about a title, independent of where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaSnapshot {
    /// The public id; becomes the primary `media_refs` row.
    pub id: MediaId,
    pub format: Option<MediaFormat>,
    pub title: String,
    pub original_title: Option<String>,
    pub romaji_title: Option<String>,
    pub alt_titles: Vec<String>,
    pub overview: Option<String>,
    pub poster_url: Option<String>,
    pub backdrop_url: Option<String>,
    pub start_date: Option<chrono::NaiveDate>,
    pub end_date: Option<chrono::NaiveDate>,
    pub status: Option<ReleaseStatus>,
    pub runtime_minutes: Option<i32>,
    pub genres: Vec<String>,
    pub origin_countries: Vec<String>,
    pub original_language: Option<String>,
    pub season_count: Option<i32>,
    pub episode_count: Option<i32>,
    pub raw: Option<serde_json::Value>,
    /// Further external ids for the same title (e.g. its IMDb id).
    pub extra_refs: Vec<(RefProvider, String)>,
}

impl MediaSnapshot {
    /// A minimal snapshot carrying only the id — what is stored when a title
    /// is tracked while its provider is unreachable.
    pub fn placeholder(id: &MediaId) -> Self {
        Self {
            id: id.clone(),
            format: None,
            title: id.to_string(),
            original_title: None,
            romaji_title: None,
            alt_titles: Vec::new(),
            overview: None,
            poster_url: None,
            backdrop_url: None,
            start_date: None,
            end_date: None,
            status: None,
            runtime_minutes: None,
            genres: Vec::new(),
            origin_countries: Vec::new(),
            original_language: None,
            season_count: None,
            episode_count: None,
            raw: None,
            extra_refs: Vec::new(),
        }
    }

    fn category(&self) -> crate::entity::enums::Category {
        derive_category(CategoryInput {
            provider: self.id.provider,
            kind: self.id.kind,
            format: self.format,
            origin_countries: &self.origin_countries,
            original_language: self.original_language.as_deref(),
            genres: &self.genres,
        })
    }

    /// The column values, leaving `id`, `created_at` and the refresh
    /// bookkeeping to the caller.
    fn fields(&self) -> media::ActiveModel {
        media::ActiveModel {
            id: NotSet,
            kind: Set(self.id.kind),
            format: Set(self.format),
            category: Set(self.category()),
            title: Set(self.title.clone()),
            original_title: Set(self.original_title.clone()),
            romaji_title: Set(self.romaji_title.clone()),
            alt_titles: Set(serde_json::json!(self.alt_titles)),
            overview: Set(self.overview.clone()),
            poster_url: Set(self.poster_url.clone()),
            backdrop_url: Set(self.backdrop_url.clone()),
            start_date: Set(self.start_date),
            end_date: Set(self.end_date),
            status: Set(self.status),
            runtime_minutes: Set(self.runtime_minutes),
            genres: Set(self.genres.clone()),
            origin_countries: Set(self.origin_countries.clone()),
            original_language: Set(self.original_language.clone()),
            season_count: Set(self.season_count),
            episode_count: Set(self.episode_count),
            raw: Set(self.raw.clone()),
            fetched_at: NotSet,
            refresh_error: NotSet,
            refresh_failures: NotSet,
            episodes_checked_at: NotSet,
            created_at: NotSet,
            updated_at: NotSet,
        }
    }
}

fn ref_condition(id: &MediaId) -> Condition {
    Condition::all()
        .add(media_ref::Column::Provider.eq(id.provider.as_ref_provider()))
        .add(media_ref::Column::Kind.eq(id.kind))
        .add(media_ref::Column::ExternalId.eq(id.external_id()))
}

/// The snapshot row a public id resolves to, if the title is in the catalog.
pub async fn find_by_ref<C: ConnectionTrait>(
    db: &C,
    id: &MediaId,
) -> Result<Option<media::Model>, DbErr> {
    let Some(r) = media_ref::Entity::find()
        .filter(ref_condition(id))
        .one(db)
        .await?
    else {
        return Ok(None);
    };
    media::Entity::find_by_id(r.media_id).one(db).await
}

/// The snapshot row linked to an external id of any kind (e.g. a MAL id an
/// import carries), if one has been linked.
pub async fn find_by_external<C: ConnectionTrait>(
    db: &C,
    provider: RefProvider,
    external_id: &str,
) -> Result<Option<media::Model>, DbErr> {
    let Some(r) = media_ref::Entity::find()
        .filter(media_ref::Column::Provider.eq(provider))
        .filter(media_ref::Column::ExternalId.eq(external_id))
        .one(db)
        .await?
    else {
        return Ok(None);
    };
    media::Entity::find_by_id(r.media_id).one(db).await
}

/// Resolve many public ids at once: `MediaId -> media.id` for the known ones.
pub async fn resolve_refs<C: ConnectionTrait>(
    db: &C,
    ids: &[MediaId],
) -> Result<HashMap<MediaId, Uuid>, DbErr> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let cond = ids
        .iter()
        .fold(Condition::any(), |c, id| c.add(ref_condition(id)));
    let rows = media_ref::Entity::find().filter(cond).all(db).await?;
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            let provider = Provider::from_ref_provider(r.provider)?;
            let numeric = r.external_id.parse().ok()?;
            Some((MediaId::new(provider, r.kind, numeric), r.media_id))
        })
        .collect())
}

/// The public id of each snapshot row: `media.id -> MediaId`.
pub async fn primary_refs<C: ConnectionTrait>(
    db: &C,
    media_ids: &[Uuid],
) -> Result<HashMap<Uuid, MediaId>, DbErr> {
    if media_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = media_ref::Entity::find()
        .filter(media_ref::Column::MediaId.is_in(media_ids.to_vec()))
        .filter(media_ref::Column::IsPrimary.eq(true))
        .all(db)
        .await?;
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            let provider = Provider::from_ref_provider(r.provider)?;
            let numeric = r.external_id.parse().ok()?;
            Some((r.media_id, MediaId::new(provider, r.kind, numeric)))
        })
        .collect())
}

/// Insert or refresh a snapshot, returning the stored row.
///
/// An existing row keeps its UUID (so every entry, list item and play that
/// points at it stays valid) and has its fields replaced by the fresh
/// snapshot. Two concurrent first inserts of the same title race on the
/// unique ref; the loser re-reads the winner's row.
pub async fn upsert_snapshot<C>(db: &C, snap: &MediaSnapshot) -> Result<media::Model, DbErr>
where
    C: ConnectionTrait + TransactionTrait,
{
    let txn = db.begin().await?;
    let stored = match find_by_ref(&txn, &snap.id).await? {
        Some(existing) => {
            let mut am = snap.fields();
            // Titles only Stacks' supplementary AniList query supplies are
            // kept when a refresh ran without it.
            if snap.romaji_title.is_none() {
                am.romaji_title = Set(existing.romaji_title.clone());
            }
            if snap.alt_titles.is_empty() {
                am.alt_titles = Set(existing.alt_titles.clone());
            }
            am.id = Set(existing.id);
            am.fetched_at = Set(Some(now()));
            am.refresh_error = Set(None);
            am.refresh_failures = Set(0);
            am.update(&txn).await?
        }
        None => {
            let mut am = snap.fields();
            am.id = Set(Uuid::now_v7());
            am.fetched_at = Set(Some(now()));
            am.refresh_failures = Set(0);
            let row = am.insert(&txn).await?;
            if let Err(e) = insert_ref(&txn, row.id, &snap.id, true).await {
                txn.rollback().await?;
                if crate::db::is_unique_violation(&e) {
                    return find_by_ref(db, &snap.id)
                        .await?
                        .ok_or_else(|| DbErr::RecordNotFound(snap.id.to_string()));
                }
                return Err(e);
            }
            row
        }
    };
    for (provider, external_id) in &snap.extra_refs {
        insert_extra_ref(&txn, stored.id, *provider, snap.id.kind, external_id).await?;
    }
    txn.commit().await?;
    Ok(stored)
}

/// Store a placeholder for a title whose provider could not be reached, so
/// tracking never waits on an upstream. A later refresh fills it in.
pub async fn insert_placeholder<C>(db: &C, id: &MediaId, error: &str) -> Result<media::Model, DbErr>
where
    C: ConnectionTrait + TransactionTrait,
{
    if let Some(existing) = find_by_ref(db, id).await? {
        return Ok(existing);
    }
    let txn = db.begin().await?;
    let snap = MediaSnapshot::placeholder(id);
    let mut am = snap.fields();
    am.id = Set(Uuid::now_v7());
    am.refresh_error = Set(Some(error.to_owned()));
    am.refresh_failures = Set(1);
    let row = am.insert(&txn).await?;
    if let Err(e) = insert_ref(&txn, row.id, id, true).await {
        txn.rollback().await?;
        if crate::db::is_unique_violation(&e) {
            return find_by_ref(db, id)
                .await?
                .ok_or_else(|| DbErr::RecordNotFound(id.to_string()));
        }
        return Err(e);
    }
    txn.commit().await?;
    Ok(row)
}

/// Snapshots due for a refresh, most overdue first: never fetched, or
/// fetched more than `fresh_for_secs` ago — or, for a show someone is
/// watching, new-episode detection not run (`episodes_checked_at`) for that
/// long. The second clause matters because a user read refreshes
/// `fetched_at` too, and a show its watchers open every day would otherwise
/// never be checked for new episodes.
///
/// A title whose refreshes keep failing backs off: it is retried no sooner
/// than `1h × 2^failures` (capped at a week) after its last attempt, so one
/// dead title cannot eat every batch.
pub async fn stale_snapshots<C: ConnectionTrait>(
    db: &C,
    fresh_for_secs: i64,
    limit: u64,
) -> Result<Vec<media::Model>, DbErr> {
    let sql = r#"
        SELECT m.* FROM media m
        CROSS JOIN LATERAL (
            SELECT m.kind = 'show' AND EXISTS (
                SELECT 1 FROM library_entries le
                WHERE le.media_id = m.id AND le.status = 'watching' AND le.deleted_at IS NULL
            ) AS watched
        ) w
        CROSS JOIN LATERAL (
            SELECT LEAST(
                COALESCE(m.fetched_at, '-infinity'::timestamptz),
                CASE WHEN w.watched
                     THEN COALESCE(m.episodes_checked_at, '-infinity'::timestamptz) END
            ) AS due_since
        ) d
        WHERE d.due_since < now() - make_interval(secs => $1)
          AND (m.refresh_failures = 0
               OR m.updated_at < now() - LEAST(
                    make_interval(hours => 1) * power(2, LEAST(m.refresh_failures, 8)),
                    make_interval(days => 7)))
        ORDER BY d.due_since, m.id
        LIMIT $2
    "#;
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    media::Entity::find()
        .from_raw_sql(sea_orm::Statement::from_sql_and_values(
            sea_orm::DbBackend::Postgres,
            sql,
            [(fresh_for_secs as f64).into(), limit.into()],
        ))
        .all(db)
        .await
}

/// Record when new-episode detection last ran for a title; `None` forgets
/// the baseline, so the next detection starts a fresh one.
pub async fn set_episodes_checked<C: ConnectionTrait>(
    db: &C,
    media_id: Uuid,
    at: Option<sea_orm::prelude::DateTimeWithTimeZone>,
) -> Result<(), DbErr> {
    media::Entity::update_many()
        .col_expr(media::Column::EpisodesCheckedAt, Expr::value(at))
        .filter(media::Column::Id.eq(media_id))
        .exec(db)
        .await
        .map(|_| ())
}

/// Advance a season's `announced_through` to `through`; it never moves
/// back.
pub async fn advance_announced_through<C: ConnectionTrait>(
    db: &C,
    media_id: Uuid,
    season_number: i32,
    through: i32,
) -> Result<(), DbErr> {
    season::Entity::update_many()
        .col_expr(
            season::Column::AnnouncedThrough,
            Expr::cust_with_values("GREATEST(announced_through, $1)", [through]),
        )
        .filter(season::Column::MediaId.eq(media_id))
        .filter(season::Column::Number.eq(season_number))
        .exec(db)
        .await
        .map(|_| ())
}

/// Record that refreshing a snapshot failed, keeping the old data.
pub async fn record_refresh_failure<C: ConnectionTrait>(
    db: &C,
    media_id: Uuid,
    error: &str,
) -> Result<(), DbErr> {
    let Some(m) = media::Entity::find_by_id(media_id).one(db).await? else {
        return Ok(());
    };
    let failures = m.refresh_failures.saturating_add(1);
    let mut am: media::ActiveModel = m.into();
    am.refresh_error = Set(Some(error.to_owned()));
    am.refresh_failures = Set(failures);
    am.update(db).await.map(|_| ())
}

async fn insert_ref<C: ConnectionTrait>(
    db: &C,
    media_id: Uuid,
    id: &MediaId,
    is_primary: bool,
) -> Result<(), DbErr> {
    media_ref::ActiveModel {
        id: Set(Uuid::now_v7()),
        media_id: Set(media_id),
        provider: Set(id.provider.as_ref_provider()),
        kind: Set(id.kind),
        external_id: Set(id.external_id()),
        is_primary: Set(is_primary),
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(db)
    .await
    .map(|_| ())
}

/// Link a further external id; an id already linked (to any title) is left
/// as it is.
async fn insert_extra_ref<C: ConnectionTrait>(
    db: &C,
    media_id: Uuid,
    provider: RefProvider,
    kind: MediaKind,
    external_id: &str,
) -> Result<(), DbErr> {
    let am = media_ref::ActiveModel {
        id: Set(Uuid::now_v7()),
        media_id: Set(media_id),
        provider: Set(provider),
        kind: Set(kind),
        external_id: Set(external_id.to_owned()),
        is_primary: Set(false),
        created_at: NotSet,
        updated_at: NotSet,
    };
    media_ref::Entity::insert(am)
        .on_conflict_do_nothing_on([
            media_ref::Column::Provider,
            media_ref::Column::Kind,
            media_ref::Column::ExternalId,
        ])
        .exec(db)
        .await
        .map(|_| ())
}

/// The episode row for `(media, season, number)`, creating the season and
/// episode rows (with no metadata yet) when they are missing.
pub async fn ensure_episode<C: ConnectionTrait>(
    db: &C,
    media_id: Uuid,
    season_number: i32,
    episode_number: i32,
) -> Result<episode::Model, DbErr> {
    season::Entity::insert(season::ActiveModel {
        id: Set(Uuid::now_v7()),
        media_id: Set(media_id),
        number: Set(season_number),
        title: Set(None),
        overview: Set(None),
        air_date: Set(None),
        poster_url: Set(None),
        episode_count: Set(None),
        announced_through: Set(None),
        created_at: NotSet,
        updated_at: NotSet,
    })
    .on_conflict_do_nothing_on([season::Column::MediaId, season::Column::Number])
    .exec(db)
    .await?;
    let season = season::Entity::find()
        .filter(season::Column::MediaId.eq(media_id))
        .filter(season::Column::Number.eq(season_number))
        .one(db)
        .await?
        .ok_or_else(|| DbErr::RecordNotFound(format!("season {season_number}")))?;

    episode::Entity::insert(episode::ActiveModel {
        id: Set(Uuid::now_v7()),
        media_id: Set(media_id),
        season_id: Set(season.id),
        number: Set(episode_number),
        absolute_number: Set(None),
        title: Set(None),
        overview: Set(None),
        air_date: Set(None),
        runtime_minutes: Set(None),
        still_url: Set(None),
        created_at: NotSet,
        updated_at: NotSet,
    })
    .on_conflict_do_nothing_on([episode::Column::SeasonId, episode::Column::Number])
    .exec(db)
    .await?;
    episode::Entity::find()
        .filter(episode::Column::SeasonId.eq(season.id))
        .filter(episode::Column::Number.eq(episode_number))
        .one(db)
        .await?
        .ok_or_else(|| DbErr::RecordNotFound(format!("episode {episode_number}")))
}

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
        air_date: Set(super::mapping::parse_date(s.air_date.as_deref())),
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

/// Store one season and its episodes, returning the episode rows that did
/// not exist before. Like [`upsert_seasons`], it only ever adds or fills in.
pub async fn upsert_season_episodes<C: ConnectionTrait>(
    db: &C,
    media_id: Uuid,
    season_info: &SeasonInfo,
    episodes: &[EpisodeInfo],
) -> Result<Vec<episode::Model>, DbErr> {
    let season = upsert_season(db, media_id, season_info).await?;
    let existing: std::collections::HashSet<i32> = episode::Entity::find()
        .filter(episode::Column::SeasonId.eq(season.id))
        .all(db)
        .await?
        .into_iter()
        .map(|e| e.number)
        .collect();
    let keep = |col: &str| Expr::cust(format!("COALESCE(excluded.{col}, episodes.{col})"));
    for e in episodes {
        let Ok(number) = i32::try_from(e.episode_number) else {
            continue;
        };
        episode::Entity::insert(episode::ActiveModel {
            id: Set(Uuid::now_v7()),
            media_id: Set(media_id),
            season_id: Set(season.id),
            number: Set(number),
            absolute_number: Set(None),
            title: Set(e.name.clone()),
            overview: Set(e.overview.clone()),
            air_date: Set(super::mapping::parse_date(e.air_date.as_deref())),
            runtime_minutes: Set(e.runtime_minutes.and_then(|r| i32::try_from(r).ok())),
            still_url: Set(e.still_url.clone()),
            created_at: NotSet,
            updated_at: NotSet,
        })
        .on_conflict(
            OnConflict::columns([episode::Column::SeasonId, episode::Column::Number])
                .value(episode::Column::Title, keep("title"))
                .value(episode::Column::Overview, keep("overview"))
                .value(episode::Column::AirDate, keep("air_date"))
                .value(episode::Column::RuntimeMinutes, keep("runtime_minutes"))
                .value(episode::Column::StillUrl, keep("still_url"))
                .to_owned(),
        )
        .exec(db)
        .await?;
    }
    Ok(episode::Entity::find()
        .filter(episode::Column::SeasonId.eq(season.id))
        .all(db)
        .await?
        .into_iter()
        .filter(|e| !existing.contains(&e.number))
        .collect())
}

/// The stored seasons of a title, in order.
pub async fn seasons_of<C: ConnectionTrait>(
    db: &C,
    media_id: Uuid,
) -> Result<Vec<season::Model>, DbErr> {
    season::Entity::find()
        .filter(season::Column::MediaId.eq(media_id))
        .order_by_asc(season::Column::Number)
        .all(db)
        .await
}

/// The stored episodes of one season of a title, in order.
pub async fn episodes_of<C: ConnectionTrait>(
    db: &C,
    media_id: Uuid,
    season_number: i32,
) -> Result<Vec<episode::Model>, DbErr> {
    let Some(season) = season::Entity::find()
        .filter(season::Column::MediaId.eq(media_id))
        .filter(season::Column::Number.eq(season_number))
        .one(db)
        .await?
    else {
        return Ok(Vec::new());
    };
    episode::Entity::find()
        .filter(episode::Column::SeasonId.eq(season.id))
        .order_by_asc(episode::Column::Number)
        .all(db)
        .await
}

/// The episode row for `(media, season, number)` if it exists.
pub async fn find_episode<C: ConnectionTrait>(
    db: &C,
    media_id: Uuid,
    season_number: i32,
    episode_number: i32,
) -> Result<Option<episode::Model>, DbErr> {
    let Some(season) = season::Entity::find()
        .filter(season::Column::MediaId.eq(media_id))
        .filter(season::Column::Number.eq(season_number))
        .one(db)
        .await?
    else {
        return Ok(None);
    };
    episode::Entity::find()
        .filter(episode::Column::SeasonId.eq(season.id))
        .filter(episode::Column::Number.eq(episode_number))
        .one(db)
        .await
}

/// `(season number, episode number)` for each episode id.
pub async fn episode_numbers<C: ConnectionTrait>(
    db: &C,
    episode_ids: &[Uuid],
) -> Result<HashMap<Uuid, (i32, i32)>, DbErr> {
    if episode_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let episodes = episode::Entity::find()
        .filter(episode::Column::Id.is_in(episode_ids.to_vec()))
        .all(db)
        .await?;
    let season_ids: Vec<Uuid> = episodes.iter().map(|e| e.season_id).collect();
    let seasons: HashMap<Uuid, i32> = season::Entity::find()
        .filter(season::Column::Id.is_in(season_ids))
        .all(db)
        .await?
        .into_iter()
        .map(|s| (s.id, s.number))
        .collect();
    Ok(episodes
        .into_iter()
        .filter_map(|e| Some((e.id, (*seasons.get(&e.season_id)?, e.number))))
        .collect())
}
