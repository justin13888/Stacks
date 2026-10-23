//! What to watch next, and what airs when: computed from the catalog's
//! stored seasons and episodes and the plays of each entry's current run.
//!
//! Both lists are per user and small (one item per show, or the episodes of
//! a few weeks), so each is computed whole in a fixed number of queries and
//! paged in memory by a keyset position, never by an offset.

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::NaiveDate;
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::enums::EntryStatus;
use crate::entity::{episode, library_entry, media, season};
use crate::media::catalog;

use super::{plays, repo};

/// The next episode of one show the user is watching.
#[derive(Debug, Clone, PartialEq)]
pub struct UpNext {
    pub entry: library_entry::Model,
    pub media: media::Model,
    pub season: i32,
    pub number: i32,
    /// The stored episode, or `None` when only the season's episode count
    /// says it exists (its details have not been fetched yet).
    pub episode: Option<episode::Model>,
    /// When the user last watched anything of the show.
    pub last_watched_at: Option<DateTimeWithTimeZone>,
}

/// Where an up-next item sits: most recently watched first, never-watched
/// last, then by entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpNextPosition {
    pub last_watched_at: Option<DateTimeWithTimeZone>,
    pub entry_id: Uuid,
}

impl Ord for UpNextPosition {
    fn cmp(&self, other: &Self) -> Ordering {
        let watched = match (&self.last_watched_at, &other.last_watched_at) {
            (Some(a), Some(b)) => b.cmp(a),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        };
        watched.then_with(|| other.entry_id.cmp(&self.entry_id))
    }
}

impl PartialOrd for UpNextPosition {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl UpNext {
    pub fn position(&self) -> UpNextPosition {
        UpNextPosition {
            last_watched_at: self.last_watched_at,
            entry_id: self.entry.id,
        }
    }
}

/// The episodes a show is known to have, outside specials, in order: the
/// stored ones, and those its stored season summaries count but no episode
/// row describes yet.
fn known_episodes(
    seasons: &[season::Model],
    episodes: &[(i32, episode::Model)],
) -> BTreeMap<(i32, i32), Option<episode::Model>> {
    let mut known: BTreeMap<(i32, i32), Option<episode::Model>> = BTreeMap::new();
    for s in seasons.iter().filter(|s| s.number >= 1) {
        for n in 1..=s.episode_count.unwrap_or(0) {
            known.insert((s.number, n), None);
        }
    }
    for (s, e) in episodes.iter().filter(|(s, _)| *s >= 1) {
        known.insert((*s, e.number), Some(e.clone()));
    }
    known
}

/// For each show the user is watching, the episode after the furthest one
/// watched in the current run (the first episode when none is), ordered by
/// [`UpNextPosition`]. A show with nothing left is not listed.
pub async fn up_next<C: ConnectionTrait>(db: &C, user_id: Uuid) -> Result<Vec<UpNext>, DbErr> {
    let entries = repo::watching_shows(db, user_id).await?;
    let entry_ids: Vec<Uuid> = entries.iter().map(|(e, _)| e.id).collect();
    let media_ids: Vec<Uuid> = entries.iter().map(|(_, m)| m.id).collect();
    let progress: HashMap<Uuid, Option<DateTimeWithTimeZone>> =
        repo::progress_many(db, entry_ids.clone())
            .await?
            .into_iter()
            .map(|p| (p.entry_id, p.last_watched_at))
            .collect();
    let runs = repo::current_runs(db, &entry_ids).await?;
    let run_ids: Vec<Uuid> = runs.values().map(|r| r.id).collect();
    let mut watched: HashMap<Uuid, HashSet<Uuid>> = HashMap::new();
    for p in plays::live_plays_in_runs(db, &run_ids).await? {
        if let Some(episode) = p.episode_id {
            watched.entry(p.run_id).or_default().insert(episode);
        }
    }
    let seasons = catalog::seasons_of_many(db, &media_ids).await?;
    let episodes = catalog::episodes_of_many(db, &media_ids).await?;

    let mut out = Vec::new();
    for (entry, media) in entries {
        let known = known_episodes(
            seasons.get(&media.id).map_or(&[][..], Vec::as_slice),
            episodes.get(&media.id).map_or(&[][..], Vec::as_slice),
        );
        let seen = runs
            .get(&entry.id)
            .and_then(|r| watched.get(&r.id))
            .cloned()
            .unwrap_or_default();
        let furthest = episodes
            .get(&media.id)
            .into_iter()
            .flatten()
            .filter(|(_, e)| seen.contains(&e.id))
            .map(|(s, e)| (*s, e.number))
            .max();
        let next = known
            .into_iter()
            .find(|(position, _)| furthest.is_none_or(|f| *position > f));
        let Some(((season, number), episode)) = next else {
            continue;
        };
        out.push(UpNext {
            last_watched_at: progress.get(&entry.id).copied().flatten(),
            entry,
            media,
            season,
            number,
            episode,
        });
    }
    out.sort_by_key(UpNext::position);
    Ok(out)
}

/// One episode airing for a show in the user's library.
#[derive(Debug, Clone, PartialEq)]
pub struct CalendarEpisode {
    pub entry: library_entry::Model,
    pub media: media::Model,
    pub season: i32,
    pub episode: episode::Model,
    pub air_date: NaiveDate,
    /// Whether the user has watched it in the entry's current run.
    pub watched: bool,
}

/// Where a calendar episode sits: by air date, then title (ignoring case),
/// then title row, season and episode.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CalendarPosition(pub NaiveDate, pub String, pub Uuid, pub i32, pub i32);

impl CalendarEpisode {
    pub fn position(&self) -> CalendarPosition {
        CalendarPosition(
            self.air_date,
            self.media.title.to_lowercase(),
            self.media.id,
            self.season,
            self.episode.number,
        )
    }
}

/// The stored episodes airing from `from` to `to` (inclusive) of every show
/// in the user's live library that they have not dropped, ordered by
/// [`CalendarPosition`].
pub async fn calendar<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<CalendarEpisode>, DbErr> {
    let entries = library_entry::Entity::find()
        .filter(library_entry::Column::UserId.eq(user_id))
        .filter(library_entry::Column::DeletedAt.is_null())
        .filter(library_entry::Column::Status.ne(EntryStatus::Dropped))
        .all(db)
        .await?;
    let entries = repo::with_media(db, entries).await?;
    let by_media: HashMap<Uuid, (library_entry::Model, media::Model)> =
        entries.into_iter().map(|(e, m)| (m.id, (e, m))).collect();
    if by_media.is_empty() {
        return Ok(Vec::new());
    }
    let airing = episode::Entity::find()
        .filter(episode::Column::MediaId.is_in(by_media.keys().copied()))
        .filter(episode::Column::AirDate.between(from, to))
        .all(db)
        .await?;
    if airing.is_empty() {
        return Ok(Vec::new());
    }
    let season_numbers: HashMap<Uuid, i32> = season::Entity::find()
        .filter(season::Column::Id.is_in(airing.iter().map(|e| e.season_id)))
        .all(db)
        .await?
        .into_iter()
        .map(|s| (s.id, s.number))
        .collect();
    let entry_ids: Vec<Uuid> = by_media.values().map(|(e, _)| e.id).collect();
    let runs = repo::current_runs(db, &entry_ids).await?;
    let run_ids: Vec<Uuid> = runs.values().map(|r| r.id).collect();
    let watched: HashSet<(Uuid, Uuid)> = plays::live_plays_in_runs(db, &run_ids)
        .await?
        .into_iter()
        .filter_map(|p| Some((p.run_id, p.episode_id?)))
        .collect();

    let mut out: Vec<CalendarEpisode> = airing
        .into_iter()
        .filter_map(|episode| {
            let (entry, media) = by_media.get(&episode.media_id)?;
            let season = *season_numbers.get(&episode.season_id)?;
            let air_date = episode.air_date?;
            let watched = runs
                .get(&entry.id)
                .is_some_and(|r| watched.contains(&(r.id, episode.id)));
            Some(CalendarEpisode {
                entry: entry.clone(),
                media: media.clone(),
                season,
                air_date,
                watched,
                episode,
            })
        })
        .collect();
    out.sort_by_key(CalendarEpisode::position);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: i64) -> Option<DateTimeWithTimeZone> {
        chrono::DateTime::from_timestamp(secs, 0).map(Into::into)
    }

    #[test]
    fn up_next_orders_recent_first_and_unwatched_last() {
        let ids: Vec<Uuid> = (0..3).map(|_| Uuid::now_v7()).collect();
        let mut positions = [
            UpNextPosition {
                last_watched_at: None,
                entry_id: ids[0],
            },
            UpNextPosition {
                last_watched_at: at(10),
                entry_id: ids[1],
            },
            UpNextPosition {
                last_watched_at: at(20),
                entry_id: ids[2],
            },
        ];
        positions.sort();
        let order: Vec<Uuid> = positions.iter().map(|p| p.entry_id).collect();
        assert_eq!(order, vec![ids[2], ids[1], ids[0]]);
    }
}
