//! AniList shows as seasons and episodes, and the replies of Stacks' own
//! AniList queries.
//!
//! AniList has no seasons and lists no episodes: it models each cour as a
//! title of its own, numbered from episode 1. Stacks gives every AniList show
//! one season, [`SEASON`], holding episodes `1..=N`, so its episodes can be
//! logged, marked up to and counted like a TMDB show's. N is the show's
//! episode total; for a show still airing with no total, the episode before
//! the next one to air; otherwise it is unknown and the season is empty. A
//! synthesised episode carries only its number, which is also its absolute
//! number: no title, air date or still.

use serde::Deserialize;

use super::models::{EpisodeInfo, SeasonInfo};
use super::provider::SeasonDetails;

/// The one season of an AniList show.
pub const SEASON: u32 = 1;

/// The most episodes a synthesised season holds. A larger count is taken
/// for a malformed reply and treated as unknown, so a bad upstream number
/// never turns into millions of episode rows.
pub const MAX_EPISODES: u32 = 10_000;

/// Stacks' own query for how many episodes an AniList show has. Workaround
/// for cameo dropping `nextAiringEpisode` (justin13888/Stacks#14).
pub const EPISODES_QUERY: &str = "query StacksAnimeEpisodes($id: Int) { \
    Media(id: $id, type: ANIME) { id episodes nextAiringEpisode { episode } } }";

/// The `data` of a reply to one of Stacks' own `Media(…)` queries.
#[derive(Debug, Deserialize)]
pub struct MediaData<T> {
    #[serde(rename = "Media")]
    pub media: Option<T>,
}

/// The fields Stacks' own AniList query adds to a title (cameo drops them).
/// Workaround for cameo gaps tracked in justin13888/Stacks#14.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AniListExtras {
    pub id_mal: Option<i64>,
    pub title: Option<AniListTitles>,
    pub synonyms: Option<Vec<String>>,
    pub country_of_origin: Option<String>,
    pub format: Option<String>,
    pub episodes: Option<i32>,
    pub duration: Option<i32>,
    pub status: Option<String>,
    pub next_airing_episode: Option<NextAiring>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct AniListTitles {
    pub romaji: Option<String>,
    pub english: Option<String>,
    pub native: Option<String>,
}

/// AniList's next episode to air, for a show still airing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
pub struct NextAiring {
    pub episode: Option<i32>,
}

/// The reply to [`EPISODES_QUERY`].
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AniListEpisodes {
    /// Required, so a `Media` of nothing but `null`s is a malformed reply
    /// rather than a show whose count is unknown.
    pub id: i64,
    pub episodes: Option<i32>,
    pub next_airing_episode: Option<NextAiring>,
}

impl AniListEpisodes {
    /// The show's episode count, as [`episode_count`] settles it.
    pub fn count(&self) -> Option<u32> {
        episode_count(self.episodes, self.next_airing_episode)
    }
}

/// How many episodes an AniList show's season holds: its `total` when
/// AniList knows it, else the episode before `next_airing` (zero when the
/// first has yet to air), else unknown.
pub fn episode_count(total: Option<i32>, next_airing: Option<NextAiring>) -> Option<u32> {
    let total = total.and_then(|n| u32::try_from(n).ok()).filter(|n| *n > 0);
    let aired = next_airing
        .and_then(|n| n.episode)
        .and_then(|n| u32::try_from(n).ok())
        .and_then(|n| n.checked_sub(1));
    total.or(aired).filter(|n| *n <= MAX_EPISODES)
}

/// The season summary of an AniList show holding `count` episodes.
pub fn season_summary(count: Option<u32>) -> SeasonInfo {
    SeasonInfo {
        season_number: SEASON,
        name: None,
        overview: None,
        air_date: None,
        poster_url: None,
        episode_count: count,
    }
}

/// The season of an AniList show holding `count` episodes, with each of
/// them; empty when the count is unknown.
pub fn season_details(count: Option<u32>) -> SeasonDetails {
    SeasonDetails {
        season: season_summary(count),
        episodes: (1..=count.unwrap_or(0))
            .map(|n| EpisodeInfo {
                season_number: SEASON,
                episode_number: n,
                absolute_number: Some(n),
                name: None,
                overview: None,
                air_date: None,
                runtime_minutes: None,
                still_url: None,
                rating: None,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn next(episode: i32) -> Option<NextAiring> {
        Some(NextAiring {
            episode: Some(episode),
        })
    }

    #[test]
    fn the_total_wins_then_the_next_airing_episode() {
        assert_eq!(episode_count(Some(26), None), Some(26));
        assert_eq!(episode_count(Some(26), next(5)), Some(26));
        assert_eq!(episode_count(None, next(5)), Some(4));
        assert_eq!(episode_count(Some(0), next(5)), Some(4));
        // The first episode has yet to air: known, and none.
        assert_eq!(episode_count(None, next(1)), Some(0));
    }

    #[test]
    fn an_unknown_or_absurd_count_is_unknown() {
        assert_eq!(episode_count(None, None), None);
        assert_eq!(
            episode_count(None, Some(NextAiring { episode: None })),
            None
        );
        assert_eq!(episode_count(Some(-3), next(0)), None);
        assert_eq!(episode_count(Some(-3), next(-2)), None);
        assert_eq!(episode_count(Some(10_000), None), Some(10_000));
        assert_eq!(episode_count(Some(10_001), None), None);
        assert_eq!(episode_count(None, next(i32::MAX)), None);
    }

    #[test]
    fn a_season_numbers_its_episodes_continuously() {
        let season = season_details(Some(3));
        assert_eq!(season.season.season_number, 1);
        assert_eq!(season.season.episode_count, Some(3));
        let numbers: Vec<(u32, u32, Option<u32>)> = season
            .episodes
            .iter()
            .map(|e| (e.season_number, e.episode_number, e.absolute_number))
            .collect();
        assert_eq!(numbers, [(1, 1, Some(1)), (1, 2, Some(2)), (1, 3, Some(3))]);
        assert!(
            season
                .episodes
                .iter()
                .all(|e| e.name.is_none() && e.air_date.is_none())
        );
    }

    #[test]
    fn an_unknown_count_is_an_empty_season() {
        let season = season_details(None);
        assert_eq!(season.season.season_number, 1);
        assert_eq!(season.season.episode_count, None);
        assert!(season.episodes.is_empty());
        assert_eq!(season_details(Some(0)).episodes.len(), 0);
    }
}
