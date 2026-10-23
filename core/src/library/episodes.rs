//! Which episodes a show has, for logging plays against them.
//!
//! The catalog's stored episodes answer when they are complete. Otherwise the
//! season is fetched through [`MediaService`], which stores what it fetches.
//! When the provider cannot answer, the stored season summary's episode count
//! stands in, so a provider outage blocks a play only for a title Stacks
//! knows nothing about.

use std::collections::BTreeSet;

use sea_orm::DatabaseConnection;

use crate::entity::media;
use crate::media::{MediaError, MediaId, MediaService, MetadataErrorKind, catalog};

/// The episode numbers of season `season` of a show, ascending, or `None`
/// when the show has no such season.
pub async fn season_episodes(
    db: &DatabaseConnection,
    media_service: &MediaService,
    row: &media::Model,
    id: &MediaId,
    season: i32,
) -> Result<Option<Vec<i32>>, MediaError> {
    let summary = catalog::seasons_of(db, row.id)
        .await?
        .into_iter()
        .find(|s| s.number == season);
    let stored: BTreeSet<i32> = catalog::episodes_of(db, row.id, season)
        .await?
        .into_iter()
        .map(|e| e.number)
        .collect();
    let expected = summary
        .as_ref()
        .and_then(|s| s.episode_count)
        .and_then(|n| usize::try_from(n).ok());
    if !stored.is_empty() && expected.is_none_or(|n| stored.len() >= n) {
        return Ok(Some(stored.into_iter().collect()));
    }
    // What the catalog alone can say: the stored episodes, and the numbers
    // the summary's count implies.
    let mut known = stored;
    if let Some(n) = summary.as_ref().and_then(|s| s.episode_count) {
        known.extend(1..=n);
    }
    let Ok(number) = u32::try_from(season) else {
        return Ok(None);
    };
    match media_service.get_episodes(id, number).await {
        Ok(list) if !list.freshness.stale => Ok(Some(
            list.episodes
                .iter()
                .filter_map(|e| i32::try_from(e.episode_number).ok())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        )),
        Ok(list) => {
            known.extend(
                list.episodes
                    .iter()
                    .filter_map(|e| i32::try_from(e.episode_number).ok()),
            );
            Ok(Some(known.into_iter().collect()))
        }
        Err(_) if !known.is_empty() => Ok(Some(known.into_iter().collect())),
        Err(MediaError::NotFound(_)) => Ok(None),
        Err(MediaError::Metadata(e)) if e.kind == MetadataErrorKind::Unsupported => Ok(None),
        Err(e) => Err(e),
    }
}
