//! Pure conversions from cameo's unified models into Stacks media models and
//! catalog snapshots. Together with [`provider`](super::provider) this is the
//! only module that names cameo types.
//!
//! Nothing here panics: an entry whose cameo `provider_id` cannot be parsed
//! into a [`MediaId`] is reported (as an error, or in a page's `unavailable`
//! list) rather than dropped silently.

use cameo::{
    Genre, Page, PartialDate, UnifiedEpisode, UnifiedMovie, UnifiedMovieDetails, UnifiedSeason,
    UnifiedSeasonDetails, UnifiedTvShow, UnifiedTvShowDetails,
};
use chrono::NaiveDate;
use serde::Deserialize;

use crate::entity::enums::{MediaFormat, RefProvider};

use super::catalog::MediaSnapshot;
use super::category::{CategoryInput, derive_category, normalize_release_status};
use super::id::{MediaId, MediaIdError, MediaKind, Provider};
use super::models::{
    EpisodeInfo, Freshness, MediaDetails, MediaWithUserState, SeasonInfo, in_production,
};
use super::provider::{MetadataPage, SeasonDetails, TitleDetails};

pub use super::anilist::{AniListExtras, AniListTitles};

/// Parse a provider `YYYY-MM-DD` date; anything else is dropped rather than
/// failing the snapshot (the original survives in `raw`).
pub fn parse_date(date: Option<&str>) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(date?, "%Y-%m-%d").ok()
}

fn full_date(date: Option<&PartialDate>) -> Option<NaiveDate> {
    date.and_then(PartialDate::to_naive_date)
}

fn date_string(date: Option<&PartialDate>) -> Option<String> {
    date.map(PartialDate::to_string)
}

/// Map cameo genres to their human-readable display names.
fn genre_names(genres: &[Genre]) -> Vec<String> {
    genres.iter().map(|g| g.name().into_owned()).collect()
}

fn media_id(provider_id: &cameo::MediaId, kind: MediaKind) -> Result<MediaId, MediaIdError> {
    MediaId::from_cameo(&provider_id.to_string(), kind)
}

/// The ISO 639-1 language Stacks assumes for an AniList country of origin.
///
/// cameo derives a language itself but passes unknown country codes through
/// as if they were language codes (`"TH"` becomes language `"TH"`); Stacks
/// only claims the languages it knows. Workaround for a cameo gap tracked in
/// justin13888/Stacks#14.
pub fn language_for_country(country: &str) -> Option<&'static str> {
    Some(match country.to_ascii_uppercase().as_str() {
        "JP" => "ja",
        "KR" => "ko",
        "CN" | "TW" | "HK" => "zh",
        "TH" => "th",
        "PH" => "tl",
        "US" | "GB" | "AU" | "CA" => "en",
        "FR" => "fr",
        _ => return None,
    })
}

/// Base [`MediaWithUserState`] for a movie. The user-state fields are left at
/// their defaults for [`enrichment`](super::enrichment) to overlay.
pub fn movie_base(m: &UnifiedMovie) -> Result<MediaWithUserState, MediaIdError> {
    let id = media_id(&m.provider_id, MediaKind::Movie)?;
    let genres = genre_names(&m.genres);
    let category = derive_category(CategoryInput {
        provider: id.provider,
        kind: MediaKind::Movie,
        format: None,
        origin_countries: &[],
        original_language: m.original_language.as_deref(),
        genres: &genres,
    });
    Ok(MediaWithUserState {
        cameo_media_id: id.to_string(),
        title: m.title.clone(),
        original_title: m.original_title.clone().filter(|o| *o != m.title),
        // cameo's items carry no romaji; a detail call's extras add it.
        romaji_title: None,
        kind: MediaKind::Movie,
        category,
        year: m.release_date.map(|d| d.year()),
        overview: m.overview.clone(),
        poster_url: m.poster_url.clone(),
        backdrop_url: m.backdrop_url.clone(),
        genres,
        rating: m.vote_average,
        episode_count: None,
        watch_status: None,
        rewatching: false,
        score: None,
        is_favorite: false,
    })
}

/// Base [`MediaWithUserState`] for a show. See [`movie_base`].
pub fn tv_base(t: &UnifiedTvShow) -> Result<MediaWithUserState, MediaIdError> {
    let id = media_id(&t.provider_id, MediaKind::Show)?;
    let genres = genre_names(&t.genres);
    let category = derive_category(CategoryInput {
        provider: id.provider,
        kind: MediaKind::Show,
        format: None,
        origin_countries: &t.origin_country,
        original_language: t.original_language.as_deref(),
        genres: &genres,
    });
    Ok(MediaWithUserState {
        cameo_media_id: id.to_string(),
        title: t.name.clone(),
        original_title: t.original_name.clone().filter(|o| *o != t.name),
        romaji_title: None,
        kind: MediaKind::Show,
        category,
        year: t.first_air_date.map(|d| d.year()),
        overview: t.overview.clone(),
        poster_url: t.poster_url.clone(),
        backdrop_url: t.backdrop_url.clone(),
        genres,
        rating: t.vote_average,
        // A list item carries no count; a detail call fills it in.
        episode_count: None,
        watch_status: None,
        rewatching: false,
        score: None,
        is_favorite: false,
    })
}

/// Map a page of results, keeping the entries that map and naming the ones
/// that do not in `unavailable`.
///
/// List results only ever become browse items, never snapshots: AniList's
/// list queries carry no episode count, duration or status, so those stay
/// unknown until a detail call fills them in (cameo gap, Stacks#14).
pub fn page<T>(
    page: Page<T>,
    map: impl Fn(&T) -> Result<MediaWithUserState, MediaIdError>,
    upstream_ref: impl Fn(&T) -> String,
) -> MetadataPage<MediaWithUserState> {
    let mut items = Vec::with_capacity(page.results.len());
    let mut unavailable = Vec::new();
    for result in &page.results {
        match map(result) {
            Ok(item) => items.push(item),
            Err(e) => {
                let r = upstream_ref(result);
                tracing::warn!(upstream_ref = %r, error = %e, "unmappable metadata entry");
                unavailable.push(r);
            }
        }
    }
    MetadataPage {
        items,
        page: page.page,
        has_next: page.has_next,
        unavailable,
    }
}

/// Map a page of movies. See [`page`].
pub fn movies_page(p: Page<UnifiedMovie>) -> MetadataPage<MediaWithUserState> {
    page(p, movie_base, |m| m.provider_id.to_string())
}

/// Map a page of shows. See [`page`].
pub fn shows_page(p: Page<UnifiedTvShow>) -> MetadataPage<MediaWithUserState> {
    page(p, tv_base, |t| t.provider_id.to_string())
}

/// Map cameo movie details to the detail view, its snapshot, and (none)
/// seasons.
pub fn movie_title(d: &UnifiedMovieDetails) -> Result<TitleDetails, MediaIdError> {
    let id = media_id(&d.movie.provider_id, MediaKind::Movie)?;
    let m = &d.movie;
    let status = normalize_release_status(d.status.as_deref());
    let mut extra_refs = Vec::new();
    if let Some(imdb) = d.imdb_id.as_deref().filter(|s| !s.is_empty()) {
        extra_refs.push((RefProvider::Imdb, imdb.to_owned()));
    }
    let original_language = match id.provider {
        Provider::AniList => d
            .production_countries
            .first()
            .and_then(|c| language_for_country(c))
            .map(str::to_owned),
        Provider::Tmdb => m.original_language.clone(),
    };
    let origin_countries = match id.provider {
        // AniList's country of origin arrives as `production_countries`.
        Provider::AniList => d.production_countries.clone(),
        // TMDB movies carry production countries as names, not codes, so the
        // origin is left empty and the category falls back to the language.
        Provider::Tmdb => Vec::new(),
    };
    let detail = MediaDetails {
        media: movie_base(m)?,
        format: Some(MediaFormat::Movie),
        tagline: d.tagline.clone(),
        runtime_minutes: d.runtime,
        status: d.status.clone(),
        homepage: d.homepage.clone(),
        imdb_id: d.imdb_id.clone(),
        number_of_seasons: None,
        in_production: None,
        freshness: Freshness::live(),
    };
    let snapshot = MediaSnapshot {
        id,
        format: Some(MediaFormat::Movie),
        title: m.title.clone(),
        original_title: m.original_title.clone(),
        romaji_title: None,
        alt_titles: Vec::new(),
        overview: m.overview.clone(),
        poster_url: m.poster_url.clone(),
        backdrop_url: m.backdrop_url.clone(),
        start_date: full_date(m.release_date.as_ref()),
        end_date: None,
        status,
        runtime_minutes: d.runtime.and_then(|r| i32::try_from(r).ok()),
        genres: genre_names(&m.genres),
        origin_countries,
        original_language,
        season_count: None,
        episode_count: None,
        raw: serde_json::to_value(d).ok(),
        extra_refs,
    };
    Ok(TitleDetails {
        detail,
        snapshot,
        seasons: Vec::new(),
    })
}

/// Map cameo show details to the detail view, its snapshot, and its season
/// summaries.
pub fn show_title(d: &UnifiedTvShowDetails) -> Result<TitleDetails, MediaIdError> {
    let id = media_id(&d.show.provider_id, MediaKind::Show)?;
    let s = &d.show;
    let status = normalize_release_status(d.status.as_deref());
    let format = match id.provider {
        Provider::AniList => d
            .media_format
            .as_deref()
            .and_then(anilist_format)
            .unwrap_or(MediaFormat::Tv),
        Provider::Tmdb => match d.type_.as_deref() {
            Some(t) if t.eq_ignore_ascii_case("miniseries") => MediaFormat::Miniseries,
            _ => MediaFormat::Tv,
        },
    };
    let original_language = match id.provider {
        Provider::AniList => s
            .origin_country
            .first()
            .and_then(|c| language_for_country(c))
            .map(str::to_owned),
        Provider::Tmdb => s.original_language.clone(),
    };
    let detail = MediaDetails {
        media: MediaWithUserState {
            episode_count: d.number_of_episodes,
            ..tv_base(s)?
        },
        format: Some(format),
        tagline: d.tagline.clone(),
        // `UnifiedTvShowDetails` carries a list of episode runtimes; surface
        // the first as the representative episode runtime.
        runtime_minutes: d.episode_run_time.first().copied(),
        status: d.status.clone(),
        homepage: d.homepage.clone(),
        imdb_id: None,
        number_of_seasons: d.number_of_seasons,
        // Never cameo's `in_production`: it defaults to `false` when TMDB
        // omits the field (Stacks#14). The status says the same thing and is
        // `None` when unknown.
        in_production: in_production(status),
        freshness: Freshness::live(),
    };
    let snapshot = MediaSnapshot {
        id,
        format: Some(format),
        title: s.name.clone(),
        original_title: s.original_name.clone(),
        romaji_title: None,
        alt_titles: Vec::new(),
        overview: s.overview.clone(),
        poster_url: s.poster_url.clone(),
        backdrop_url: s.backdrop_url.clone(),
        start_date: full_date(s.first_air_date.as_ref()),
        end_date: full_date(d.last_air_date.as_ref()),
        status,
        runtime_minutes: d
            .episode_run_time
            .first()
            .and_then(|r| i32::try_from(*r).ok()),
        genres: genre_names(&s.genres),
        origin_countries: s.origin_country.clone(),
        original_language,
        season_count: d.number_of_seasons.and_then(|n| i32::try_from(n).ok()),
        episode_count: d.number_of_episodes.and_then(|n| i32::try_from(n).ok()),
        raw: serde_json::to_value(d).ok(),
        extra_refs: Vec::new(),
    };
    Ok(TitleDetails {
        detail,
        snapshot,
        seasons: d.seasons.iter().map(season_summary).collect(),
    })
}

/// A season summary from a show's detail payload.
pub fn season_summary(s: &UnifiedSeason) -> SeasonInfo {
    SeasonInfo {
        season_number: s.season_number,
        name: s.name.clone(),
        overview: s.overview.clone(),
        air_date: date_string(s.air_date.as_ref()),
        poster_url: s.poster_url.clone(),
        episode_count: s.episode_count,
    }
}

/// Map cameo season details to a season summary plus its episodes.
pub fn season_details(s: &UnifiedSeasonDetails) -> SeasonDetails {
    let count = u32::try_from(s.episodes.len()).ok();
    SeasonDetails {
        season: SeasonInfo {
            season_number: s.season_number,
            name: s.name.clone(),
            overview: s.overview.clone(),
            air_date: date_string(s.air_date.as_ref()),
            poster_url: s.poster_url.clone(),
            episode_count: count,
        },
        episodes: s
            .episodes
            .iter()
            .map(|e| episode_info(e, s.season_number))
            .collect(),
    }
}

/// Map a cameo episode to an [`EpisodeInfo`]. The `season_number` must be
/// supplied by the caller — `UnifiedEpisode` does not carry it.
pub fn episode_info(e: &UnifiedEpisode, season_number: u32) -> EpisodeInfo {
    EpisodeInfo {
        season_number,
        episode_number: e.episode_number,
        // Only TMDB lists seasons, and it numbers episodes within a season
        // with no absolute number. AniList's continuous numbering is filled
        // in where its episodes are stored (`catalog::ensure_episode`).
        absolute_number: None,
        name: e.name.clone(),
        overview: e.overview.clone(),
        air_date: date_string(e.air_date.as_ref()),
        runtime_minutes: e.runtime,
        still_url: e.still_url.clone(),
        rating: e.vote_average,
    }
}

/// An AniList `MediaFormat` as a Stacks [`MediaFormat`].
fn anilist_format(raw: &str) -> Option<MediaFormat> {
    Some(match raw.to_ascii_uppercase().as_str() {
        "TV" => MediaFormat::Tv,
        "TV_SHORT" => MediaFormat::TvShort,
        "MOVIE" => MediaFormat::Movie,
        "OVA" => MediaFormat::Ova,
        "ONA" => MediaFormat::Ona,
        "SPECIAL" => MediaFormat::Special,
        _ => return None,
    })
}

/// One page of Stacks' own AniList list query (the airing feed).
///
/// Entries stay raw JSON until [`anilist_list_page`] maps them one by one, so
/// a single malformed entry is named in `unavailable` instead of failing the
/// page; a missing `pageInfo` still fails it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AniListListPage {
    pub page_info: AniListPageInfo,
    pub media: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AniListPageInfo {
    pub has_next_page: bool,
}

/// One entry of [`AniListListPage`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AniListListMedia {
    id: i64,
    title: Option<AniListTitles>,
    description: Option<String>,
    start_date: Option<AniListYear>,
    cover_image: Option<AniListCover>,
    banner_image: Option<String>,
    genres: Option<Vec<String>>,
    average_score: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
struct AniListYear {
    year: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AniListCover {
    large: Option<String>,
    extra_large: Option<String>,
}

/// Map one page of Stacks' AniList list query, the way cameo maps its own
/// AniList lists: the English title before the romaji one, the native title
/// as the original, the largest cover, and the 0–100 score on a 0–10 scale.
/// The query asked for `kind`'s formats only, so every entry is that kind.
pub fn anilist_list_page(
    p: AniListListPage,
    kind: MediaKind,
    page: u32,
) -> MetadataPage<MediaWithUserState> {
    let mut items = Vec::with_capacity(p.media.len());
    let mut unavailable = Vec::new();
    for raw in &p.media {
        match anilist_list_item(raw, kind) {
            Ok(item) => items.push(item),
            Err(e) => {
                let r = raw.get("id").map_or_else(
                    || "anilist:unknown".to_owned(),
                    |id| format!("anilist:{id}"),
                );
                tracing::warn!(upstream_ref = %r, error = %e, "unmappable metadata entry");
                unavailable.push(r);
            }
        }
    }
    MetadataPage {
        items,
        page,
        has_next: p.page_info.has_next_page,
        unavailable,
    }
}

fn anilist_list_item(
    raw: &serde_json::Value,
    kind: MediaKind,
) -> Result<MediaWithUserState, String> {
    let m = AniListListMedia::deserialize(raw).map_err(|e| e.to_string())?;
    let id = u64::try_from(m.id)
        .map_err(|e| e.to_string())
        .and_then(|n| media_id(&cameo::MediaId::anilist(n), kind).map_err(|e| e.to_string()))?;
    let titles = m.title.unwrap_or_default();
    let title = titles
        .english
        .clone()
        .or_else(|| titles.romaji.clone())
        .filter(|t| !t.is_empty())
        .ok_or_else(|| "no title".to_owned())?;
    let genres: Vec<String> = m
        .genres
        .unwrap_or_default()
        .iter()
        .map(|g| Genre::from_anilist_genre(g).name().into_owned())
        .collect();
    let category = derive_category(CategoryInput {
        provider: id.provider,
        kind,
        format: None,
        origin_countries: &[],
        original_language: None,
        genres: &genres,
    });
    Ok(MediaWithUserState {
        cameo_media_id: id.to_string(),
        original_title: titles.native.filter(|o| *o != title),
        romaji_title: titles.romaji.filter(|r| !r.is_empty()),
        title,
        kind,
        category,
        year: m.start_date.and_then(|d| d.year),
        overview: m.description,
        poster_url: m.cover_image.and_then(|c| c.extra_large.or(c.large)),
        backdrop_url: m.banner_image,
        genres,
        rating: m.average_score.map(|s| f64::from(s) / 10.0),
        episode_count: None,
        watch_status: None,
        rewatching: false,
        score: None,
        is_favorite: false,
    })
}

/// Fold the extras into a snapshot. A field the extras leave out keeps the
/// value cameo's detail call produced.
pub fn apply_anilist_extras(snap: &mut MediaSnapshot, extras: &AniListExtras) {
    if let Some(titles) = &extras.title {
        if let Some(romaji) = titles.romaji.clone().filter(|s| !s.is_empty()) {
            snap.romaji_title = Some(romaji);
        }
        let mut alt: Vec<String> = Vec::new();
        let candidates = [titles.english.as_ref(), titles.native.as_ref()]
            .into_iter()
            .flatten()
            .chain(extras.synonyms.iter().flatten());
        for t in candidates {
            if !t.is_empty() && *t != snap.title && !alt.contains(t) {
                alt.push(t.clone());
            }
        }
        if !alt.is_empty() {
            snap.alt_titles = alt;
        }
    }
    if let Some(mal) = extras.id_mal.filter(|id| *id > 0) {
        let mal = mal.to_string();
        if !snap
            .extra_refs
            .iter()
            .any(|(p, v)| *p == RefProvider::Mal && *v == mal)
        {
            snap.extra_refs.push((RefProvider::Mal, mal));
        }
    }
    if let Some(country) = extras
        .country_of_origin
        .as_deref()
        .filter(|c| !c.is_empty())
    {
        snap.origin_countries = vec![country.to_owned()];
        snap.original_language = language_for_country(country).map(str::to_owned);
    }
    if snap.id.kind == MediaKind::Show
        && let Some(format) = extras.format.as_deref().and_then(anilist_format)
    {
        snap.format = Some(format);
    }
    if let Some(n) = extras.episodes.filter(|n| *n >= 0) {
        snap.episode_count = Some(n);
    }
    if let Some(d) = extras.duration.filter(|d| *d > 0) {
        snap.runtime_minutes = Some(d);
    }
    if let Some(status) = normalize_release_status(extras.status.as_deref()) {
        snap.status = Some(status);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn country_language_is_stacks_owned() {
        assert_eq!(language_for_country("JP"), Some("ja"));
        assert_eq!(language_for_country("kr"), Some("ko"));
        assert_eq!(language_for_country("TW"), Some("zh"));
        // Unknown countries claim no language rather than echoing the code.
        assert_eq!(language_for_country("XX"), None);
    }

    #[test]
    fn extras_fill_romaji_mal_and_origin() {
        let mut snap =
            MediaSnapshot::placeholder(&MediaId::new(Provider::AniList, MediaKind::Show, 21));
        snap.title = "One Piece".into();
        let extras = AniListExtras {
            id_mal: Some(21),
            title: Some(AniListTitles {
                romaji: Some("ONE PIECE".into()),
                english: Some("One Piece".into()),
                native: Some("ワンピース".into()),
            }),
            synonyms: Some(vec!["OP".into(), "OP".into()]),
            country_of_origin: Some("JP".into()),
            format: Some("TV".into()),
            episodes: None,
            duration: Some(24),
            status: Some("RELEASING".into()),
            next_airing_episode: None,
        };
        apply_anilist_extras(&mut snap, &extras);
        assert_eq!(snap.romaji_title.as_deref(), Some("ONE PIECE"));
        assert_eq!(
            snap.alt_titles,
            vec!["ワンピース".to_owned(), "OP".to_owned()]
        );
        assert_eq!(snap.extra_refs, vec![(RefProvider::Mal, "21".to_owned())]);
        assert_eq!(snap.origin_countries, vec!["JP".to_owned()]);
        assert_eq!(snap.original_language.as_deref(), Some("ja"));
        assert_eq!(snap.format, Some(MediaFormat::Tv));
        assert_eq!(snap.episode_count, None, "an absent count is not invented");
        assert_eq!(snap.runtime_minutes, Some(24));
        assert_eq!(
            snap.status,
            Some(crate::entity::enums::ReleaseStatus::Releasing)
        );

        // Applying twice does not duplicate the MAL ref.
        apply_anilist_extras(&mut snap, &extras);
        assert_eq!(snap.extra_refs.len(), 1);
    }
}
