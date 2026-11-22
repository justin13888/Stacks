//! Pure conversions from cameo's unified models into Stacks media models and
//! catalog snapshots. Together with [`provider`](super::provider) this is the
//! only module that names cameo types.
//!
//! Nothing here panics: an entry whose cameo `provider_id` cannot be parsed
//! into a [`MediaId`] is reported (as an error, or in a page's `unavailable`
//! list) rather than dropped silently.

use cameo::{
    AlternativeTitle, Genre, Page, PartialDate, SeriesStatus, SeriesType, TitleKind,
    UnifiedEpisode, UnifiedMovie, UnifiedMovieDetails, UnifiedSeason, UnifiedSeasonDetails,
    UnifiedTvShow, UnifiedTvShowDetails,
};
use chrono::NaiveDate;

use crate::entity::enums::{MediaFormat, RefProvider};

use super::catalog::MediaSnapshot;
use super::category::{CategoryInput, derive_category, normalize_release_status};
use super::id::{MediaId, MediaIdError, MediaKind, Provider};
use super::models::{
    EpisodeInfo, Freshness, MediaDetails, MediaWithUserState, SeasonInfo, in_production,
};
use super::provider::{MetadataPage, SeasonDetails, TitleDetails};

pub use super::anilist::AniListExtras;

/// Parse a provider `YYYY-MM-DD` date; anything else is dropped rather than
/// failing the snapshot (the original survives in `raw`).
pub fn parse_date(date: Option<&str>) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(date?, "%Y-%m-%d").ok()
}

/// A complete provider date as a calendar date; a year-only or year-month
/// date is no date.
fn full_date(date: Option<&PartialDate>) -> Option<NaiveDate> {
    let d = date?;
    NaiveDate::from_ymd_opt(d.year(), u32::from(d.month()?), u32::from(d.day()?))
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

/// The ISO 639-1 language Stacks assumes for an AniList country of origin
/// when cameo names none.
///
/// cameo maps only the four common anime origins (JP, CN, KR, TW) and leaves
/// any other country without a language; this table also covers the other
/// origins Stacks' categories tell apart.
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

/// A title's original language: cameo's when it names one, else the one
/// [`language_for_country`] assumes for an AniList title's origin.
fn original_language(
    provider: Provider,
    cameo: Option<&str>,
    origin_countries: &[String],
) -> Option<String> {
    match (cameo, provider) {
        (Some(lang), _) => Some(lang.to_owned()),
        (None, Provider::AniList) => origin_countries
            .first()
            .and_then(|c| language_for_country(c))
            .map(str::to_owned),
        (None, Provider::Tmdb) => None,
    }
}

/// The romanized title among a title's variants (AniList's romaji).
fn romaji_title(alternatives: &[AlternativeTitle]) -> Option<String> {
    alternatives
        .iter()
        .find(|a| a.kind == TitleKind::Romanized)
        .map(|a| a.title.clone())
}

/// A title's other names: every variant but the romanized one (kept as the
/// romaji title) and the headline title, each once, in cameo's order.
fn alt_titles(title: &str, alternatives: &[AlternativeTitle]) -> Vec<String> {
    let mut alt: Vec<String> = Vec::new();
    for a in alternatives {
        if a.kind != TitleKind::Romanized && a.title != title && !alt.contains(&a.title) {
            alt.push(a.title.clone());
        }
    }
    alt
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
        romaji_title: romaji_title(&m.alternative_titles),
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
        romaji_title: romaji_title(&t.alternative_titles),
        kind: MediaKind::Show,
        category,
        year: t.first_air_date.map(|d| d.year()),
        overview: t.overview.clone(),
        poster_url: t.poster_url.clone(),
        backdrop_url: t.backdrop_url.clone(),
        genres,
        rating: t.vote_average,
        // AniList's lists carry the count; TMDB's do not, so a detail call
        // fills it in.
        episode_count: t.number_of_episodes,
        watch_status: None,
        rewatching: false,
        score: None,
        is_favorite: false,
    })
}

/// Map a page of results, keeping the entries that map and naming the ones
/// that do not in `unavailable`.
///
/// List results only ever become browse items, never snapshots: a list
/// entry is thinner than its details (TMDB's carry no episode count), so the
/// snapshot waits for a detail call.
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
    // cameo hands a movie's status over as the provider's own word, untyped
    // (justin13888/cameo#196), so Stacks normalises it.
    let status = normalize_release_status(d.status.as_deref());
    let mut extra_refs = Vec::new();
    if let Some(imdb) = d.imdb_id.as_deref().filter(|s| !s.is_empty()) {
        extra_refs.push((RefProvider::Imdb, imdb.to_owned()));
    }
    let origin_countries = match id.provider {
        // AniList's country of origin arrives as `production_countries`.
        Provider::AniList => d.production_countries.clone(),
        // TMDB movies carry production countries as names, not codes, so the
        // origin is left empty and the category falls back to the language.
        Provider::Tmdb => Vec::new(),
    };
    let original_language = original_language(
        id.provider,
        m.original_language.as_deref(),
        &origin_countries,
    );
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
        romaji_title: romaji_title(&m.alternative_titles),
        alt_titles: alt_titles(&m.title, &m.alternative_titles),
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

/// A show's status as its provider words it, which is what
/// [`MediaDetails::status`] publishes and [`normalize_release_status`] reads.
///
/// cameo hands the status over parsed; the words are the provider's own
/// (TMDB's "Returning Series", AniList's "RELEASING"), so the published value
/// is the one the provider sent.
fn series_status_word(provider: Provider, status: &SeriesStatus) -> String {
    let anilist = provider == Provider::AniList;
    let word = match status {
        SeriesStatus::ReturningSeries if anilist => "RELEASING",
        SeriesStatus::ReturningSeries => "Returning Series",
        SeriesStatus::Planned if anilist => "NOT_YET_RELEASED",
        SeriesStatus::Planned => "Planned",
        SeriesStatus::InProduction => "In Production",
        SeriesStatus::Ended if anilist => "FINISHED",
        SeriesStatus::Ended => "Ended",
        SeriesStatus::Cancelled if anilist => "CANCELLED",
        SeriesStatus::Cancelled => "Canceled",
        SeriesStatus::Pilot => "Pilot",
        SeriesStatus::Hiatus => "HIATUS",
        SeriesStatus::Other(word) => return word.clone(),
        other => return format!("{other:?}"),
    };
    word.to_owned()
}

/// Map cameo show details to the detail view, its snapshot, and its season
/// summaries.
pub fn show_title(d: &UnifiedTvShowDetails) -> Result<TitleDetails, MediaIdError> {
    let id = media_id(&d.show.provider_id, MediaKind::Show)?;
    let s = &d.show;
    let status_word = d
        .status
        .as_ref()
        .map(|st| series_status_word(id.provider, st));
    let status = normalize_release_status(status_word.as_deref());
    let format = match id.provider {
        Provider::AniList => d
            .media_format
            .as_deref()
            .and_then(anilist_format)
            .unwrap_or(MediaFormat::Tv),
        Provider::Tmdb => match d.type_ {
            Some(SeriesType::Miniseries) => MediaFormat::Miniseries,
            _ => MediaFormat::Tv,
        },
    };
    let original_language = original_language(
        id.provider,
        s.original_language.as_deref(),
        &s.origin_country,
    );
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
        status: status_word,
        homepage: d.homepage.clone(),
        imdb_id: None,
        number_of_seasons: d.number_of_seasons,
        // From the release status, not cameo's `in_production`: TMDB's flag
        // is `false` between seasons of a returning show, AniList's (cameo
        // derives it) is `false` on hiatus, and a stale snapshot, which has
        // no flag, must say what a live read says.
        in_production: in_production(status),
        freshness: Freshness::live(),
    };
    let snapshot = MediaSnapshot {
        id,
        format: Some(format),
        title: s.name.clone(),
        original_title: s.original_name.clone(),
        romaji_title: romaji_title(&s.alternative_titles),
        alt_titles: alt_titles(&s.name, &s.alternative_titles),
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
    SeasonDetails {
        season: SeasonInfo {
            season_number: s.season_number,
            name: s.name.clone(),
            overview: s.overview.clone(),
            air_date: date_string(s.air_date.as_ref()),
            poster_url: s.poster_url.clone(),
            episode_count: u32::try_from(s.episodes.len()).ok(),
        },
        episodes: s
            .episodes
            .iter()
            .map(|e| episode_info(e, s.season_number))
            .collect(),
        next_airing: None,
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

/// Fold the extras into a snapshot: the MAL id, as a ref.
pub fn apply_anilist_extras(snap: &mut MediaSnapshot, extras: &AniListExtras) {
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
    fn an_unknown_language_falls_back_to_the_anilist_origin_only() {
        let th = ["TH".to_owned()];
        assert_eq!(
            original_language(Provider::AniList, None, &th).as_deref(),
            Some("th")
        );
        assert_eq!(
            original_language(Provider::AniList, Some("ja"), &th).as_deref(),
            Some("ja"),
            "cameo's language wins"
        );
        assert_eq!(original_language(Provider::Tmdb, None, &th), None);
    }

    #[test]
    fn title_variants_fill_romaji_and_alt_titles() {
        let alts = vec![
            AlternativeTitle::new("One Piece", TitleKind::English),
            AlternativeTitle::new("ONE PIECE", TitleKind::Romanized),
            AlternativeTitle::new("ワンピース", TitleKind::Native),
            AlternativeTitle::new("OP", TitleKind::Synonym),
            AlternativeTitle::new("OP", TitleKind::Other),
        ];
        assert_eq!(romaji_title(&alts).as_deref(), Some("ONE PIECE"));
        assert_eq!(
            alt_titles("One Piece", &alts),
            vec!["ワンピース".to_owned(), "OP".to_owned()]
        );
        assert_eq!(romaji_title(&[]), None);
    }

    #[test]
    fn extras_add_the_mal_ref_once() {
        let mut snap =
            MediaSnapshot::placeholder(&MediaId::new(Provider::AniList, MediaKind::Show, 21));
        let extras = AniListExtras {
            id_mal: Some(21),
            next_airing_episode: None,
        };
        apply_anilist_extras(&mut snap, &extras);
        assert_eq!(snap.extra_refs, vec![(RefProvider::Mal, "21".to_owned())]);
        // Applying twice does not duplicate the MAL ref.
        apply_anilist_extras(&mut snap, &extras);
        assert_eq!(snap.extra_refs.len(), 1);
        // A non-positive MAL id is no ref.
        let mut snap =
            MediaSnapshot::placeholder(&MediaId::new(Provider::AniList, MediaKind::Show, 21));
        apply_anilist_extras(
            &mut snap,
            &AniListExtras {
                id_mal: Some(0),
                next_airing_episode: None,
            },
        );
        assert!(snap.extra_refs.is_empty());
    }

    #[test]
    fn only_a_complete_partial_date_is_a_date() {
        let full: PartialDate = "2010-07-16".parse().expect("date");
        let year: PartialDate = "2010".parse().expect("date");
        let month: PartialDate = "2010-07".parse().expect("date");
        assert_eq!(full_date(Some(&full)), NaiveDate::from_ymd_opt(2010, 7, 16));
        assert_eq!(full_date(Some(&year)), None);
        assert_eq!(full_date(Some(&month)), None);
        assert_eq!(full_date(None), None);
        // A day past the month's end parses but is no calendar date.
        let feb30 = PartialDate::ymd(2010, 2, 30).expect("in range");
        assert_eq!(full_date(Some(&feb30)), None);
    }

    #[test]
    fn a_series_status_is_published_in_its_providers_words() {
        use cameo::SeriesStatus as S;
        let word = |p, s: S| series_status_word(p, &s);
        assert_eq!(word(Provider::AniList, S::ReturningSeries), "RELEASING");
        assert_eq!(word(Provider::AniList, S::Hiatus), "HIATUS");
        assert_eq!(word(Provider::Tmdb, S::Cancelled), "Canceled");
        assert_eq!(word(Provider::Tmdb, S::Other("Rumored".into())), "Rumored");
    }
}
