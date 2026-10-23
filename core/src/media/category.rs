//! Pure derivations stored on the catalog snapshot: the browsing category and
//! the normalised release status.

use crate::entity::enums::{Category, MediaFormat, MediaKind, ReleaseStatus};

use super::id::Provider;

/// Everything [`derive_category`] looks at.
#[derive(Debug, Clone, Copy)]
pub struct CategoryInput<'a> {
    pub provider: Provider,
    pub kind: MediaKind,
    pub format: Option<MediaFormat>,
    /// ISO 3166-1 alpha-2 codes, any case.
    pub origin_countries: &'a [String],
    /// ISO 639-1 code, any case.
    pub original_language: Option<&'a str>,
    /// Display names, any case.
    pub genres: &'a [String],
}

/// Genres that make a show something other than a scripted drama, whatever
/// its country.
const NON_DRAMA_GENRES: &[&str] = &["animation", "reality", "talk", "news", "documentary"];

/// Derive the browsing category of a title.
///
/// The rules, in order:
///
/// 1. Anything from AniList is anime, as is any OVA/ONA.
/// 2. Japanese animation (by origin country or original language) is anime.
/// 3. Any other movie is a movie.
/// 4. A scripted show from Korea, Japan, China (incl. Hong Kong), Taiwan or
///    Thailand is that country's drama; animation, reality, talk, news and
///    documentary shows are not dramas.
/// 5. Everything else is a series.
pub fn derive_category(input: CategoryInput<'_>) -> Category {
    let has_genre = |name: &str| input.genres.iter().any(|g| g.eq_ignore_ascii_case(name));
    let from_country = |code: &str| {
        input
            .origin_countries
            .iter()
            .any(|c| c.eq_ignore_ascii_case(code))
    };
    let language = |code: &str| {
        input
            .original_language
            .is_some_and(|l| l.eq_ignore_ascii_case(code))
    };

    if input.provider == Provider::AniList
        || matches!(input.format, Some(MediaFormat::Ova | MediaFormat::Ona))
    {
        return Category::Anime;
    }
    if has_genre("animation") && (from_country("JP") || language("ja")) {
        return Category::Anime;
    }
    if input.kind == MediaKind::Movie {
        return Category::Movie;
    }
    if NON_DRAMA_GENRES.iter().any(|g| has_genre(g)) {
        return Category::Series;
    }
    let drama = [
        ("KR", "ko", Category::KDrama),
        ("JP", "ja", Category::JDrama),
        ("CN", "zh", Category::CDrama),
        ("HK", "cn", Category::CDrama),
        ("TW", "", Category::TwDrama),
        ("TH", "th", Category::ThDrama),
    ];
    // Origin country decides first; the language is a fallback for providers
    // that omit the country.
    if let Some((_, _, c)) = drama.iter().find(|(country, _, _)| from_country(country)) {
        return *c;
    }
    if input.origin_countries.is_empty()
        && let Some((_, _, c)) = drama
            .iter()
            .find(|(_, lang, _)| !lang.is_empty() && language(lang))
    {
        return *c;
    }
    Category::Series
}

/// Normalise a provider's free-text release status.
///
/// Covers TMDB's movie and TV vocabularies and AniList's enum. Unknown values
/// map to `None` (the original survives in `media.raw`).
pub fn normalize_release_status(raw: Option<&str>) -> Option<ReleaseStatus> {
    let s = raw?.trim().to_ascii_lowercase().replace(['_', '-'], " ");
    Some(match s.as_str() {
        "rumored" | "planned" | "in production" | "post production" | "pilot"
        | "not yet released" => ReleaseStatus::Upcoming,
        "returning series" | "releasing" | "airing" => ReleaseStatus::Releasing,
        "released" | "ended" | "finished" => ReleaseStatus::Finished,
        "canceled" | "cancelled" => ReleaseStatus::Cancelled,
        "hiatus" => ReleaseStatus::Hiatus,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Case {
        name: &'static str,
        provider: Provider,
        kind: MediaKind,
        format: Option<MediaFormat>,
        countries: &'static [&'static str],
        language: Option<&'static str>,
        genres: &'static [&'static str],
        want: Category,
    }

    const CASES: &[Case] = &[
        Case {
            name: "any AniList title is anime",
            provider: Provider::AniList,
            kind: MediaKind::Show,
            format: Some(MediaFormat::Tv),
            countries: &[],
            language: None,
            genres: &[],
            want: Category::Anime,
        },
        Case {
            name: "AniList movie is anime",
            provider: Provider::AniList,
            kind: MediaKind::Movie,
            format: Some(MediaFormat::Movie),
            countries: &["JP"],
            language: Some("ja"),
            genres: &["Drama"],
            want: Category::Anime,
        },
        Case {
            name: "an OVA is anime whatever the provider",
            provider: Provider::Tmdb,
            kind: MediaKind::Show,
            format: Some(MediaFormat::Ova),
            countries: &[],
            language: None,
            genres: &[],
            want: Category::Anime,
        },
        Case {
            name: "Japanese animated TMDB show is anime",
            provider: Provider::Tmdb,
            kind: MediaKind::Show,
            format: None,
            countries: &["JP"],
            language: Some("ja"),
            genres: &["Animation", "Action & Adventure"],
            want: Category::Anime,
        },
        Case {
            name: "Japanese animated TMDB movie is anime (language only)",
            provider: Provider::Tmdb,
            kind: MediaKind::Movie,
            format: None,
            countries: &[],
            language: Some("ja"),
            genres: &["animation"],
            want: Category::Anime,
        },
        Case {
            name: "American animated movie is a movie",
            provider: Provider::Tmdb,
            kind: MediaKind::Movie,
            format: None,
            countries: &["US"],
            language: Some("en"),
            genres: &["Animation", "Family"],
            want: Category::Movie,
        },
        Case {
            name: "Korean movie is a movie, not a drama",
            provider: Provider::Tmdb,
            kind: MediaKind::Movie,
            format: None,
            countries: &["KR"],
            language: Some("ko"),
            genres: &["Drama"],
            want: Category::Movie,
        },
        Case {
            name: "Korean scripted show is a k-drama",
            provider: Provider::Tmdb,
            kind: MediaKind::Show,
            format: None,
            countries: &["KR"],
            language: Some("ko"),
            genres: &["Drama", "Romance"],
            want: Category::KDrama,
        },
        Case {
            name: "Japanese live-action show is a j-drama",
            provider: Provider::Tmdb,
            kind: MediaKind::Show,
            format: None,
            countries: &["jp"],
            language: Some("ja"),
            genres: &["Comedy"],
            want: Category::JDrama,
        },
        Case {
            name: "Chinese show is a c-drama",
            provider: Provider::Tmdb,
            kind: MediaKind::Show,
            format: None,
            countries: &["CN"],
            language: Some("zh"),
            genres: &["Drama"],
            want: Category::CDrama,
        },
        Case {
            name: "Hong Kong show is a c-drama",
            provider: Provider::Tmdb,
            kind: MediaKind::Show,
            format: None,
            countries: &["HK"],
            language: Some("cn"),
            genres: &[],
            want: Category::CDrama,
        },
        Case {
            name: "Taiwanese show is a tw-drama",
            provider: Provider::Tmdb,
            kind: MediaKind::Show,
            format: None,
            countries: &["TW"],
            language: Some("zh"),
            genres: &["Drama"],
            want: Category::TwDrama,
        },
        Case {
            name: "Thai show is a th-drama",
            provider: Provider::Tmdb,
            kind: MediaKind::Show,
            format: None,
            countries: &["TH"],
            language: Some("th"),
            genres: &["Drama"],
            want: Category::ThDrama,
        },
        Case {
            name: "Korean reality show is a series",
            provider: Provider::Tmdb,
            kind: MediaKind::Show,
            format: None,
            countries: &["KR"],
            language: Some("ko"),
            genres: &["Reality"],
            want: Category::Series,
        },
        Case {
            name: "Korean animated show is a series",
            provider: Provider::Tmdb,
            kind: MediaKind::Show,
            format: None,
            countries: &["KR"],
            language: Some("ko"),
            genres: &["Animation"],
            want: Category::Series,
        },
        Case {
            name: "language decides only when no country is given",
            provider: Provider::Tmdb,
            kind: MediaKind::Show,
            format: None,
            countries: &[],
            language: Some("ko"),
            genres: &["Drama"],
            want: Category::KDrama,
        },
        Case {
            name: "country wins over language",
            provider: Provider::Tmdb,
            kind: MediaKind::Show,
            format: None,
            countries: &["US"],
            language: Some("ko"),
            genres: &["Drama"],
            want: Category::Series,
        },
        Case {
            name: "American show is a series",
            provider: Provider::Tmdb,
            kind: MediaKind::Show,
            format: None,
            countries: &["US"],
            language: Some("en"),
            genres: &["Drama", "Crime"],
            want: Category::Series,
        },
        Case {
            name: "show with no metadata is a series",
            provider: Provider::Tmdb,
            kind: MediaKind::Show,
            format: None,
            countries: &[],
            language: None,
            genres: &[],
            want: Category::Series,
        },
    ];

    #[test]
    fn category_table() {
        for case in CASES {
            let countries: Vec<String> = case.countries.iter().map(|s| s.to_string()).collect();
            let genres: Vec<String> = case.genres.iter().map(|s| s.to_string()).collect();
            let got = derive_category(CategoryInput {
                provider: case.provider,
                kind: case.kind,
                format: case.format,
                origin_countries: &countries,
                original_language: case.language,
                genres: &genres,
            });
            assert_eq!(got, case.want, "{}", case.name);
        }
    }

    #[test]
    fn release_status_table() {
        let cases = [
            (Some("Released"), Some(ReleaseStatus::Finished)),
            (Some("Ended"), Some(ReleaseStatus::Finished)),
            (Some("FINISHED"), Some(ReleaseStatus::Finished)),
            (Some("Returning Series"), Some(ReleaseStatus::Releasing)),
            (Some("RELEASING"), Some(ReleaseStatus::Releasing)),
            (Some("In Production"), Some(ReleaseStatus::Upcoming)),
            (Some("NOT_YET_RELEASED"), Some(ReleaseStatus::Upcoming)),
            (Some("Canceled"), Some(ReleaseStatus::Cancelled)),
            (Some("CANCELLED"), Some(ReleaseStatus::Cancelled)),
            (Some("HIATUS"), Some(ReleaseStatus::Hiatus)),
            (Some("something new"), None),
            (None, None),
        ];
        for (raw, want) in cases {
            assert_eq!(normalize_release_status(raw), want, "{raw:?}");
        }
    }
}
