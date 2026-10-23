//! Pure conversions from cameo's unified models into Stacks media models and
//! catalog snapshots.
//!
//! These functions are infallible except where a cameo `provider_id` cannot be
//! parsed into a [`MediaId`] — which should not happen for well-formed cameo
//! responses, but is surfaced as an error rather than panicking.

use cameo::{
    Genre, UnifiedEpisode, UnifiedMovie, UnifiedMovieDetails, UnifiedSeasonDetails, UnifiedTvShow,
    UnifiedTvShowDetails,
};
use chrono::NaiveDate;

use crate::entity::enums::{MediaFormat, RefProvider};

use super::catalog::MediaSnapshot;
use super::category::normalize_release_status;
use super::id::{MediaId, MediaIdError, MediaKind};
use super::models::{EpisodeInfo, MediaDetails, MediaWithUserState, SeasonInfo};

/// Extract a 4-digit year from a `YYYY-MM-DD` date string.
fn year_of(date: Option<&str>) -> Option<i32> {
    date.and_then(|d| d.get(0..4)).and_then(|y| y.parse().ok())
}

/// Parse a provider `YYYY-MM-DD` date; anything else is dropped rather than
/// failing the snapshot (the original survives in `raw`).
pub fn parse_date(date: Option<&str>) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(date?, "%Y-%m-%d").ok()
}

/// Map cameo genres to their human-readable display names.
fn genre_names(genres: &[Genre]) -> Vec<String> {
    genres.iter().map(|g| g.name().to_owned()).collect()
}

/// Base [`MediaWithUserState`] for a movie. The user-state fields are left at
/// their defaults for [`enrichment`](super::enrichment) to overlay.
pub fn movie_base(m: &UnifiedMovie) -> Result<MediaWithUserState, MediaIdError> {
    let id = MediaId::from_cameo(&m.provider_id, MediaKind::Movie)?;
    Ok(MediaWithUserState {
        cameo_media_id: id.to_string(),
        title: m.title.clone(),
        kind: MediaKind::Movie,
        year: year_of(m.release_date.as_deref()),
        overview: m.overview.clone(),
        poster_url: m.poster_url.clone(),
        backdrop_url: m.backdrop_url.clone(),
        genres: genre_names(&m.genres),
        rating: m.vote_average,
        watch_status: None,
        rewatching: false,
        score: None,
        is_favorite: false,
    })
}

/// Base [`MediaWithUserState`] for a show. See [`movie_base`].
pub fn tv_base(t: &UnifiedTvShow) -> Result<MediaWithUserState, MediaIdError> {
    let id = MediaId::from_cameo(&t.provider_id, MediaKind::Show)?;
    Ok(MediaWithUserState {
        cameo_media_id: id.to_string(),
        title: t.name.clone(),
        kind: MediaKind::Show,
        year: year_of(t.first_air_date.as_deref()),
        overview: t.overview.clone(),
        poster_url: t.poster_url.clone(),
        backdrop_url: t.backdrop_url.clone(),
        genres: genre_names(&t.genres),
        rating: t.vote_average,
        watch_status: None,
        rewatching: false,
        score: None,
        is_favorite: false,
    })
}

/// Map a slice of cameo movies to base media items.
pub fn movies_base(movies: &[UnifiedMovie]) -> Result<Vec<MediaWithUserState>, MediaIdError> {
    movies.iter().map(movie_base).collect()
}

/// Map a slice of cameo shows to base media items.
pub fn tv_shows_base(shows: &[UnifiedTvShow]) -> Result<Vec<MediaWithUserState>, MediaIdError> {
    shows.iter().map(tv_base).collect()
}

/// Map cameo movie details to [`MediaDetails`].
pub fn movie_details(d: &UnifiedMovieDetails) -> Result<MediaDetails, MediaIdError> {
    Ok(MediaDetails {
        media: movie_base(&d.movie)?,
        tagline: d.tagline.clone(),
        runtime_minutes: d.runtime,
        status: d.status.clone(),
        homepage: d.homepage.clone(),
        imdb_id: d.imdb_id.clone(),
        number_of_seasons: None,
        number_of_episodes: None,
        in_production: None,
    })
}

/// Map cameo show details to [`MediaDetails`].
pub fn tv_details(d: &UnifiedTvShowDetails) -> Result<MediaDetails, MediaIdError> {
    Ok(MediaDetails {
        media: tv_base(&d.show)?,
        tagline: d.tagline.clone(),
        // `UnifiedTvShowDetails` carries a list of episode runtimes; surface
        // the first as the representative episode runtime.
        runtime_minutes: d.episode_run_time.first().copied(),
        status: d.status.clone(),
        homepage: d.homepage.clone(),
        imdb_id: None,
        number_of_seasons: Some(d.number_of_seasons),
        number_of_episodes: Some(d.number_of_episodes),
        in_production: Some(d.in_production),
    })
}

/// A catalog snapshot of a movie, keeping the full provider payload in `raw`.
pub fn movie_snapshot(d: &UnifiedMovieDetails) -> Result<MediaSnapshot, MediaIdError> {
    let id = MediaId::from_cameo(&d.movie.provider_id, MediaKind::Movie)?;
    let m = &d.movie;
    let mut extra_refs = Vec::new();
    if let Some(imdb) = d.imdb_id.as_deref().filter(|s| !s.is_empty()) {
        extra_refs.push((RefProvider::Imdb, imdb.to_owned()));
    }
    Ok(MediaSnapshot {
        id,
        format: Some(MediaFormat::Movie),
        title: m.title.clone(),
        original_title: m.original_title.clone(),
        romaji_title: None,
        alt_titles: Vec::new(),
        overview: m.overview.clone(),
        poster_url: m.poster_url.clone(),
        backdrop_url: m.backdrop_url.clone(),
        start_date: parse_date(m.release_date.as_deref()),
        end_date: None,
        status: normalize_release_status(d.status.as_deref()),
        runtime_minutes: d.runtime.and_then(|r| i32::try_from(r).ok()),
        genres: genre_names(&m.genres),
        // TMDB movies carry production countries as names, not codes, so the
        // origin is left empty and the category falls back to the language.
        origin_countries: Vec::new(),
        original_language: m.original_language.clone(),
        season_count: None,
        episode_count: None,
        raw: serde_json::to_value(d).ok(),
        extra_refs,
    })
}

/// A catalog snapshot of a show, keeping the full provider payload in `raw`.
pub fn show_snapshot(d: &UnifiedTvShowDetails) -> Result<MediaSnapshot, MediaIdError> {
    let id = MediaId::from_cameo(&d.show.provider_id, MediaKind::Show)?;
    let s = &d.show;
    let format = match d.type_.as_deref() {
        Some(t) if t.eq_ignore_ascii_case("miniseries") => MediaFormat::Miniseries,
        _ => MediaFormat::Tv,
    };
    Ok(MediaSnapshot {
        id,
        format: Some(format),
        title: s.name.clone(),
        original_title: s.original_name.clone(),
        romaji_title: None,
        alt_titles: Vec::new(),
        overview: s.overview.clone(),
        poster_url: s.poster_url.clone(),
        backdrop_url: s.backdrop_url.clone(),
        start_date: parse_date(s.first_air_date.as_deref()),
        end_date: parse_date(d.last_air_date.as_deref()),
        status: normalize_release_status(d.status.as_deref()),
        runtime_minutes: d
            .episode_run_time
            .first()
            .and_then(|r| i32::try_from(*r).ok()),
        genres: genre_names(&s.genres),
        origin_countries: s.origin_country.clone(),
        original_language: s.original_language.clone(),
        season_count: i32::try_from(d.number_of_seasons).ok(),
        episode_count: i32::try_from(d.number_of_episodes).ok(),
        raw: serde_json::to_value(d).ok(),
        extra_refs: Vec::new(),
    })
}

/// Map cameo season details to a [`SeasonInfo`] summary.
pub fn season_info(s: &UnifiedSeasonDetails) -> SeasonInfo {
    SeasonInfo {
        season_number: s.season_number,
        name: s.name.clone(),
        overview: s.overview.clone(),
        air_date: s.air_date.clone(),
        poster_url: s.poster_url.clone(),
        episode_count: s.episodes.len() as u32,
    }
}

/// Map a cameo episode to an [`EpisodeInfo`]. The `season_number` must be
/// supplied by the caller — `UnifiedEpisode` does not carry it.
pub fn episode_info(e: &UnifiedEpisode, season_number: u32) -> EpisodeInfo {
    EpisodeInfo {
        season_number,
        episode_number: e.episode_number,
        name: e.name.clone(),
        overview: e.overview.clone(),
        air_date: e.air_date.clone(),
        runtime_minutes: e.runtime,
        still_url: e.still_url.clone(),
        rating: e.vote_average,
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_date, year_of};

    #[test]
    fn year_extraction() {
        assert_eq!(year_of(Some("2010-07-16")), Some(2010));
        assert_eq!(year_of(Some("1999")), Some(1999));
        assert_eq!(year_of(Some("20xy-01-01")), None);
        assert_eq!(year_of(Some("bad")), None);
        assert_eq!(year_of(Some("")), None);
        assert_eq!(year_of(None), None);
    }

    #[test]
    fn date_parsing_drops_garbage() {
        assert_eq!(
            parse_date(Some("2010-07-16")),
            chrono::NaiveDate::from_ymd_opt(2010, 7, 16)
        );
        assert_eq!(parse_date(Some("2010")), None);
        assert_eq!(parse_date(Some("")), None);
        assert_eq!(parse_date(None), None);
    }
}
