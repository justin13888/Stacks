//! Pure conversions from cameo's unified models into Stacks media models.
//!
//! These functions are infallible except where a cameo `provider_id` cannot be
//! parsed into a [`MediaId`] — which should not happen for well-formed cameo
//! responses, but is surfaced as an error rather than panicking.

use cameo::{
    Genre, UnifiedEpisode, UnifiedMovie, UnifiedMovieDetails, UnifiedSeasonDetails, UnifiedTvShow,
    UnifiedTvShowDetails,
};

use super::id::{MediaId, MediaIdError, MediaKind};
use super::models::{AvailabilityInfo, EpisodeInfo, MediaDetails, MediaWithUserState, SeasonInfo};

/// Extract a 4-digit year from a `YYYY-MM-DD` date string.
fn year_of(date: Option<&str>) -> Option<i32> {
    date.and_then(|d| d.get(0..4)).and_then(|y| y.parse().ok())
}

/// Map cameo genres to their human-readable display names.
fn genre_names(genres: &[Genre]) -> Vec<String> {
    genres.iter().map(|g| g.name().to_owned()).collect()
}

/// Base [`MediaWithUserState`] for a movie. The user-state fields
/// (`watch_status`, `score`, `is_favorite`, `availability`) are left at their
/// defaults for [`enrichment`](super::enrichment) to overlay.
pub fn movie_base(m: &UnifiedMovie) -> Result<MediaWithUserState, MediaIdError> {
    let id = MediaId::from_cameo(&m.provider_id, MediaKind::Movie)?;
    Ok(MediaWithUserState {
        cameo_media_id: id.to_storage_string(),
        title: m.title.clone(),
        media_type: MediaKind::Movie.as_str().to_owned(),
        year: year_of(m.release_date.as_deref()),
        overview: m.overview.clone(),
        poster_url: m.poster_url.clone(),
        backdrop_url: m.backdrop_url.clone(),
        genres: genre_names(&m.genres),
        rating: m.vote_average,
        watch_status: None,
        score: None,
        is_favorite: false,
        availability: AvailabilityInfo::default(),
    })
}

/// Base [`MediaWithUserState`] for a TV show. See [`movie_base`].
pub fn tv_base(t: &UnifiedTvShow) -> Result<MediaWithUserState, MediaIdError> {
    let id = MediaId::from_cameo(&t.provider_id, MediaKind::Tv)?;
    Ok(MediaWithUserState {
        cameo_media_id: id.to_storage_string(),
        title: t.name.clone(),
        media_type: MediaKind::Tv.as_str().to_owned(),
        year: year_of(t.first_air_date.as_deref()),
        overview: t.overview.clone(),
        poster_url: t.poster_url.clone(),
        backdrop_url: t.backdrop_url.clone(),
        genres: genre_names(&t.genres),
        rating: t.vote_average,
        watch_status: None,
        score: None,
        is_favorite: false,
        availability: AvailabilityInfo::default(),
    })
}

/// Map a slice of cameo movies to base media items.
pub fn movies_base(movies: &[UnifiedMovie]) -> Result<Vec<MediaWithUserState>, MediaIdError> {
    movies.iter().map(movie_base).collect()
}

/// Map a slice of cameo TV shows to base media items.
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

/// Map cameo TV show details to [`MediaDetails`].
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
    use super::year_of;

    #[test]
    fn year_extraction() {
        assert_eq!(year_of(Some("2010-07-16")), Some(2010));
        assert_eq!(year_of(Some("1999")), Some(1999));
        assert_eq!(year_of(Some("20xy-01-01")), None);
        assert_eq!(year_of(Some("bad")), None);
        assert_eq!(year_of(Some("")), None);
        assert_eq!(year_of(None), None);
    }
}
