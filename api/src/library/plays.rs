//! Logging plays, and undoing them.

use std::sync::Arc;

use kynos::extract::link::Link;
use kynos::prelude::*;
use sea_orm::DatabaseConnection;
use stacks_core::entity::enums::PlaySource;
use stacks_core::entity::media;
use stacks_core::library::episodes::season_episodes;
use stacks_core::library::{LibraryError, LibraryService, PlayBatch};
use stacks_core::media::{MediaError, MediaId, MediaKind, MediaService, MetadataProvider};

use super::entries::{TitleUnavailable, ensure_title, get_library_entry};
use super::{
    EntryPath, EpisodeNotFound, LoggedPlays, NewPlays, Play, PlayAlreadyVoided, PlayNotFound,
    PlayNotVoided, PlayPath, WrongPlayKind, unexpected,
};
use crate::problem::{CatalogError, Internal, TitleNotFound, Upstream};
use crate::security::{Caller, Writer};

/// What logging plays can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum LogPlaysError {
    #[error(transparent)]
    #[problem(transparent)]
    TitleNotFound(#[from] TitleNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    EpisodeNotFound(#[from] EpisodeNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    WrongPlayKind(#[from] WrongPlayKind),
    #[error(transparent)]
    #[problem(transparent)]
    Upstream(#[from] Upstream),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

impl From<TitleUnavailable> for LogPlaysError {
    fn from(e: TitleUnavailable) -> Self {
        match e {
            TitleUnavailable::NotFound(e) => e.into(),
            TitleUnavailable::Internal(e) => e.into(),
        }
    }
}

impl From<MediaError> for LogPlaysError {
    fn from(e: MediaError) -> Self {
        match CatalogError::from(e) {
            // The title is stored by now; a season the provider does not
            // know reads as no season rather than as an error.
            CatalogError::NotFound(TitleNotFound(what)) => EpisodeNotFound(what).into(),
            CatalogError::Upstream(u) => u.into(),
            CatalogError::Internal(i) => i.into(),
        }
    }
}

/// What voiding a play can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum VoidPlayError {
    #[error(transparent)]
    #[problem(transparent)]
    NotFound(#[from] PlayNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    AlreadyVoided(#[from] PlayAlreadyVoided),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What restoring a play can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum RestorePlayError {
    #[error(transparent)]
    #[problem(transparent)]
    NotFound(#[from] PlayNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    NotVoided(#[from] PlayNotVoided),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// Season `season`'s episode numbers, or a 404 naming the episode asked for.
async fn episodes_of(
    db: &DatabaseConnection,
    media_service: &MediaService,
    row: &media::Model,
    id: &MediaId,
    season: i32,
    asked: (u32, u32),
) -> Result<Vec<i32>, LogPlaysError> {
    season_episodes(db, media_service, row, id, season)
        .await?
        .ok_or_else(|| EpisodeNotFound(format!("S{}E{}", asked.0, asked.1)).into())
}

fn to_i32(n: u32) -> i32 {
    // Bounded by the schema's `maximum`.
    i32::try_from(n).unwrap_or(i32::MAX)
}

/// Log plays of a movie or of a show's episodes.
///
/// Tracks the title (as `watching`) when it is not in the library. A movie
/// play completes it, and so do the plays that finish a show that has ended
/// or been cancelled: every episode of every regular season (specials aside)
/// watched in the current run, once per run: a run that finished before
/// (whatever status the user set since) is not completed again. The run's
/// completion date is then a movie's `watched_on`, or else the UTC date of
/// `watched_at` (a run that started later starts then too), or today when
/// undated. `episodes` logs one play of each listed episode;
/// `through` marks every episode of the current run not yet watched, from
/// season 1 episode 1 up to and including the one given. All the plays are
/// one change: either every one is logged or none is.
#[kynos::post("/library/{ref}/plays", operation_id = "logPlays")]
pub(super) async fn log_plays(
    Auth(caller): Writer,
    Path(path): Path<EntryPath>,
    entry_link: Link<get_library_entry>,
    Inject(db): Inject<DatabaseConnection>,
    Inject(metadata): Inject<Arc<dyn MetadataProvider>>,
    Json(body): Json<NewPlays>,
) -> Result<Created<Json<LoggedPlays>>, LogPlaysError> {
    let caller = Caller::from(caller);
    let id = path.reference.media_id().clone();
    match (&body, id.kind) {
        (NewPlays::Movie { .. }, MediaKind::Show) => {
            return Err(WrongPlayKind(
                "a show's plays are of its episodes; log `episodes` or `through`",
            )
            .into());
        }
        (NewPlays::Episodes { .. } | NewPlays::Through { .. }, MediaKind::Movie) => {
            return Err(WrongPlayKind("a movie has no episodes; log a `movie` play").into());
        }
        _ => {}
    }
    let row = ensure_title(&db, metadata.clone(), &path.reference).await?;
    let media_service = MediaService::new(db.clone(), metadata);
    let now = || Some(stacks_core::db::now());
    let mut on = None;
    let (batch, watched_at, source) = match body {
        NewPlays::Movie {
            watched_at,
            watched_on,
        } => {
            on = watched_on.map(Into::into);
            (
                PlayBatch::Movie,
                watched_at.map(Into::into).or_else(now),
                PlaySource::Manual,
            )
        }
        NewPlays::Episodes {
            episodes,
            watched_at,
        } => {
            let mut known: std::collections::HashMap<i32, Vec<i32>> =
                std::collections::HashMap::new();
            let mut wanted = Vec::with_capacity(episodes.len());
            for e in &episodes {
                let (season, number) = (to_i32(e.season), to_i32(e.episode));
                if let std::collections::hash_map::Entry::Vacant(slot) = known.entry(season) {
                    slot.insert(
                        episodes_of(
                            &db,
                            &media_service,
                            &row,
                            &id,
                            season,
                            (e.season, e.episode),
                        )
                        .await?,
                    );
                }
                if !known.get(&season).is_some_and(|n| n.contains(&number)) {
                    return Err(EpisodeNotFound(format!("S{}E{}", e.season, e.episode)).into());
                }
                wanted.push((season, number));
            }
            (
                PlayBatch::Episodes(wanted),
                watched_at.map(Into::into).or_else(now),
                PlaySource::Manual,
            )
        }
        NewPlays::Through {
            season,
            episode,
            watched_at,
        } => {
            let (last_season, last) = (to_i32(season), to_i32(episode));
            // The last season first: a season the show does not have is a
            // 404 after one lookup, not after one per season below it.
            let numbers = episodes_of(
                &db,
                &media_service,
                &row,
                &id,
                last_season,
                (season, episode),
            )
            .await?;
            if !numbers.contains(&last) {
                return Err(EpisodeNotFound(format!("S{season}E{episode}")).into());
            }
            // The seasons before it the catalog lists, or, for a title
            // stored without its seasons, every number below it.
            let listed = stacks_core::media::catalog::seasons_of(&db, row.id)
                .await
                .map_err(Internal::from)?;
            let earlier: Vec<i32> = if listed.is_empty() {
                (1..last_season).collect()
            } else {
                listed
                    .into_iter()
                    .map(|s| s.number)
                    .filter(|s| (1..last_season).contains(s))
                    .collect()
            };
            let mut wanted = Vec::new();
            for s in earlier {
                if let Some(numbers) = season_episodes(&db, &media_service, &row, &id, s).await? {
                    wanted.extend(numbers.into_iter().map(|n| (s, n)));
                }
            }
            wanted.extend(
                numbers
                    .into_iter()
                    .filter(|n| *n <= last)
                    .map(|n| (last_season, n)),
            );
            (
                PlayBatch::Unwatched(wanted),
                watched_at.map(Into::into),
                PlaySource::Bulk,
            )
        }
    };
    let service = LibraryService::new(db.clone());
    let logged = service
        .log_plays_on(caller.id(), &row, batch, watched_at, on, source)
        .await
        .map_err(|e| match e {
            LibraryError::NotAShow | LibraryError::NotAMovie => {
                LogPlaysError::from(WrongPlayKind("the play kind does not fit the title"))
            }
            other => unexpected(other).into(),
        })?;
    let runs = service
        .runs(caller.id(), row.id)
        .await
        .map_err(unexpected)?;
    let episode_ids: Vec<_> = logged.iter().filter_map(|p| p.episode_id).collect();
    let numbers = stacks_core::media::catalog::episode_numbers(&db, &episode_ids)
        .await
        .map_err(Internal::from)?;
    let plays = logged
        .iter()
        .map(|p| {
            let run = runs
                .iter()
                .find(|r| r.id == p.run_id)
                .map_or(0, |r| r.run_number);
            Play::new(p, run, p.episode_id.and_then(|e| numbers.get(&e).copied()))
        })
        .collect();
    let location = entry_link.uri(EntryPath {
        reference: path.reference,
    });
    Ok(Created::at(location, Json(LoggedPlays { plays })))
}

/// Undo a play.
///
/// The play is voided, never deleted: it stops counting towards progress
/// and stays in the entry's history, and `restorePlay` brings it back.
#[kynos::delete("/plays/{id}", operation_id = "voidPlay")]
pub(super) async fn void_play(
    Auth(caller): Writer,
    Path(path): Path<PlayPath>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<NoContent, VoidPlayError> {
    let caller = Caller::from(caller);
    LibraryService::new(db)
        .void_play(caller.id(), path.id)
        .await
        .map_err(|e| match e {
            LibraryError::PlayNotFound => VoidPlayError::from(PlayNotFound),
            LibraryError::AlreadyVoided => PlayAlreadyVoided.into(),
            other => unexpected(other).into(),
        })?;
    Ok(NoContent)
}

/// Restore a voided play.
///
/// Undoes `voidPlay` exactly: the same play (its id, run, episode, time and
/// source) counts towards progress again. The restoration is recorded in
/// the entry's history.
#[kynos::post("/plays/{id}/restorations", operation_id = "restorePlay")]
pub(super) async fn restore_play(
    Auth(caller): Writer,
    Path(path): Path<PlayPath>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<Json<Play>, RestorePlayError> {
    let caller = Caller::from(caller);
    let play = LibraryService::new(db.clone())
        .restore_play(caller.id(), path.id)
        .await
        .map_err(|e| match e {
            LibraryError::PlayNotFound => RestorePlayError::from(PlayNotFound),
            LibraryError::NotVoided => PlayNotVoided.into(),
            other => unexpected(other).into(),
        })?;
    // Runs are never deleted, so a play's run is always there.
    let run = stacks_core::library::repo::find_run(&db, play.run_id)
        .await
        .map_err(Internal::from)?
        .ok_or_else(|| Internal::log("a restored play without its run"))?;
    let episodes: Vec<_> = play.episode_id.into_iter().collect();
    let numbers = stacks_core::media::catalog::episode_numbers(&db, &episodes)
        .await
        .map_err(Internal::from)?;
    let episode = play.episode_id.and_then(|e| numbers.get(&e).copied());
    Ok(Json(Play::new(&play, run.run_number, episode)))
}
