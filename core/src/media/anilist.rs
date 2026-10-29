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
//! number: no title or still, and no air date, except the next episode to
//! air, dated from its broadcast time when AniList has one. An undated
//! episode before it has aired; one after it has not (see [`aired`]).

use chrono::{DateTime, NaiveDate};
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
    Media(id: $id, type: ANIME) { id episodes nextAiringEpisode { episode airingAt } } }";

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
#[serde(rename_all = "camelCase")]
pub struct NextAiring {
    pub episode: Option<i32>,
    /// When it airs, in seconds since the Unix epoch.
    pub airing_at: Option<i64>,
}

impl NextAiring {
    /// The UTC date the episode airs, when AniList gives a usable time.
    pub fn date(&self) -> Option<NaiveDate> {
        DateTime::from_timestamp(self.airing_at?, 0).map(|t| t.date_naive())
    }
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

    /// The show's one season: [`season_details`] of its [`count`](Self::count),
    /// with the next episode to air dated when the season lists it.
    pub fn season(&self) -> SeasonDetails {
        let mut season = season_details(self.count());
        if let Some(next) = self.next_airing_episode
            && let Some(number) = next.episode.and_then(|n| u32::try_from(n).ok())
            && let Some(date) = next.date()
            && let Some(episode) = season
                .episodes
                .iter_mut()
                .find(|e| e.episode_number == number)
        {
            episode.air_date = Some(date.format("%Y-%m-%d").to_string());
        }
        season
    }
}

/// The numbers of a synthesised season's episodes that have aired by
/// `today`. AniList dates only the next episode to air, and every episode
/// before it has aired: a dated episode has aired when its date is not
/// after `today`, and an undated one when it comes before the first dated
/// episode, or when none is dated.
pub fn aired(episodes: &[EpisodeInfo], today: NaiveDate) -> Vec<u32> {
    let date = |e: &EpisodeInfo| {
        e.air_date
            .as_deref()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
    };
    let next = episodes
        .iter()
        .filter(|e| date(e).is_some())
        .map(|e| e.episode_number)
        .min();
    episodes
        .iter()
        .filter(|e| match date(e) {
            Some(d) => d <= today,
            None => next.is_none_or(|next| e.episode_number < next),
        })
        .map(|e| e.episode_number)
        .collect()
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
            airing_at: None,
        })
    }

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("date")
    }

    fn episodes(count: Option<i32>, next_episode: i32, airing_at: Option<i64>) -> AniListEpisodes {
        AniListEpisodes {
            id: 1,
            episodes: count,
            next_airing_episode: Some(NextAiring {
                episode: Some(next_episode),
                airing_at,
            }),
        }
    }

    #[test]
    fn the_reply_carries_the_next_airing_time() {
        let reply: AniListEpisodes = serde_json::from_value(serde_json::json!({
            "id": 1, "episodes": 12,
            "nextAiringEpisode": { "episode": 5, "airingAt": 1_794_063_600 },
        }))
        .expect("reply");
        assert_eq!(reply, episodes(Some(12), 5, Some(1_794_063_600)));
    }

    #[test]
    fn the_next_episode_is_dated_from_its_airing_time() {
        // 2026-11-07T15:00:00Z.
        let season = episodes(Some(12), 5, Some(1_794_063_600)).season();
        assert_eq!(season.episodes.len(), 12);
        let dated: Vec<(u32, &str)> = season
            .episodes
            .iter()
            .filter_map(|e| Some((e.episode_number, e.air_date.as_deref()?)))
            .collect();
        assert_eq!(dated, [(5, "2026-11-07")]);
        // Without a total the next episode is not listed, so nothing is dated.
        let season = episodes(None, 5, Some(1_794_063_600)).season();
        assert_eq!(season.episodes.len(), 4);
        assert!(season.episodes.iter().all(|e| e.air_date.is_none()));
        // An unusable time dates nothing.
        let season = episodes(Some(12), 5, Some(i64::MAX)).season();
        assert!(season.episodes.iter().all(|e| e.air_date.is_none()));
    }

    #[test]
    fn undated_episodes_before_the_next_to_air_have_aired() {
        let today = day("2026-11-01");
        let airing = episodes(Some(12), 5, Some(1_794_063_600)).season();
        assert_eq!(aired(&airing.episodes, today), [1, 2, 3, 4]);
        // Once its date comes, the dated episode has aired too, and the
        // undated ones after it still have not.
        assert_eq!(aired(&airing.episodes, day("2026-11-07")), [1, 2, 3, 4, 5]);
        // Nothing dated ahead: every listed episode has aired.
        let finished = season_details(Some(3));
        assert_eq!(aired(&finished.episodes, today), [1, 2, 3]);
        let mut first = season_details(Some(2));
        first.episodes[0].air_date = Some("2026-11-02".into());
        assert!(aired(&first.episodes, today).is_empty());
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
        assert_eq!(episode_count(None, Some(NextAiring::default())), None);
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
