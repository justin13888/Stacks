//! A smart list's filter: the stored [`FilterQuery`] read once into the
//! types its evaluator uses, and the predicate a candidate must pass.

use std::collections::HashSet;
use std::ops::RangeInclusive;

use uuid::Uuid;

use crate::entity::enums::{Category, EntryStatus};
use crate::media::DiscoverCategory;
use crate::media::filter::FilterQuery;
use crate::media::id::{MediaKind, parse_kind};
use crate::media::models::MediaWithUserState;

use super::ListError;

/// A smart list's filter, read and checked: the criteria of a stored
/// [`FilterQuery`], each in the type the evaluator uses.
#[derive(Debug, Clone, PartialEq)]
pub struct SmartFilter {
    /// The kinds the list draws from, each once; never empty (both when the
    /// stored filter names none).
    pub kinds: Vec<MediaKind>,
    pub genres: Option<Vec<String>>,
    pub categories: Option<Vec<Category>>,
    pub year_min: Option<i32>,
    pub year_max: Option<i32>,
    pub statuses: Option<Vec<EntryStatus>>,
    /// Tag ids; a title passes when its library entry carries any of them.
    pub tags: Option<Vec<Uuid>>,
    /// The provider rating bounds, 0 to 10.
    pub score_min: Option<i32>,
    pub score_max: Option<i32>,
    /// The bounds on the user's own score, 0 to 100.
    pub user_score_min: Option<i32>,
    pub user_score_max: Option<i32>,
    /// Where candidates come from.
    pub source: Source,
}

/// Where a smart list's candidates come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// A TMDB discovery feed.
    Feed(DiscoverCategory),
    /// The user's own library: every live entry, most recently changed
    /// first.
    Library,
}

impl Default for SmartFilter {
    /// The empty filter: both kinds of the popular feed, unnarrowed.
    fn default() -> Self {
        Self {
            kinds: vec![MediaKind::Movie, MediaKind::Show],
            genres: None,
            categories: None,
            year_min: None,
            year_max: None,
            statuses: None,
            tags: None,
            score_min: None,
            score_max: None,
            user_score_min: None,
            user_score_max: None,
            source: Source::Feed(DiscoverCategory::Popular),
        }
    }
}

impl SmartFilter {
    /// Whether the filter names tags to narrow by.
    pub fn has_tags(&self) -> bool {
        self.tags.as_ref().is_some_and(|t| !t.is_empty())
    }
}

/// The source a stored `sort` names.
fn sort_source(sort: &str) -> Option<Source> {
    match sort {
        "popular" => Some(Source::Feed(DiscoverCategory::Popular)),
        "top_rated" | "rating" | "score" => Some(Source::Feed(DiscoverCategory::TopRated)),
        "trending" => Some(Source::Feed(DiscoverCategory::Trending)),
        "library" => Some(Source::Library),
        _ => None,
    }
}

/// A pair of bounds, checked to lie within `range` and in order.
fn bounds(
    (lo, hi): (Option<i32>, Option<i32>),
    range: RangeInclusive<i32>,
    outside: &str,
    reversed: &str,
) -> Result<(), ListError> {
    if [lo, hi].into_iter().flatten().any(|b| !range.contains(&b)) {
        return Err(ListError::InvalidFilter(outside.to_owned()));
    }
    if let (Some(lo), Some(hi)) = (lo, hi)
        && lo > hi
    {
        return Err(ListError::InvalidFilter(reversed.to_owned()));
    }
    Ok(())
}

/// A stored filter, read and checked into a [`SmartFilter`].
pub fn parse_filter(raw: &serde_json::Value) -> Result<SmartFilter, ListError> {
    let filter: FilterQuery =
        serde_json::from_value(raw.clone()).map_err(|e| ListError::InvalidFilter(e.to_string()))?;
    check_filter(filter)
}

/// A typed filter, checked into a [`SmartFilter`].
pub fn check_filter(filter: FilterQuery) -> Result<SmartFilter, ListError> {
    let invalid = |why: &str| ListError::InvalidFilter(why.to_owned());
    let mut kinds = Vec::new();
    for raw in filter.media_type.iter().flatten() {
        let kind =
            parse_kind(raw).map_err(|_| invalid("`media_type` holds `movie`, `show` or `tv`"))?;
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    if kinds.is_empty() {
        kinds = SmartFilter::default().kinds;
    }
    let statuses = filter
        .status
        .map(|statuses| {
            statuses
                .iter()
                .map(|s| {
                    EntryStatus::parse(s).ok_or_else(|| invalid("`status` holds library statuses"))
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    let categories = filter
        .category
        .map(|categories| {
            categories
                .iter()
                .map(|c| Category::parse(c).ok_or_else(|| invalid("`category` holds categories")))
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    let source = match filter.sort.as_deref() {
        None => SmartFilter::default().source,
        Some(sort) => sort_source(sort).ok_or_else(|| {
            invalid("`sort` is `popular`, `top_rated`, `rating`, `score`, `trending` or `library`")
        })?,
    };
    if let (Some(lo), Some(hi)) = (filter.year_min, filter.year_max)
        && lo > hi
    {
        return Err(invalid("`year_min` is after `year_max`"));
    }
    bounds(
        (filter.score_min, filter.score_max),
        0..=10,
        "ratings are 0 to 10",
        "the minimum rating is above the maximum",
    )?;
    bounds(
        (filter.user_score_min, filter.user_score_max),
        0..=100,
        "scores are 0 to 100",
        "the minimum score is above the maximum",
    )?;
    Ok(SmartFilter {
        kinds,
        genres: filter.genre,
        categories,
        year_min: filter.year_min,
        year_max: filter.year_max,
        statuses,
        tags: filter.tag,
        score_min: filter.score_min,
        score_max: filter.score_max,
        user_score_min: filter.user_score_min,
        user_score_max: filter.user_score_max,
        source,
    })
}

/// Whether `value` passes the optional bounds `lo` and `hi`; a missing value
/// fails any bound.
fn within<T: PartialOrd + Copy>(value: Option<T>, lo: Option<T>, hi: Option<T>) -> bool {
    lo.is_none_or(|lo| value.is_some_and(|v| v >= lo))
        && hi.is_none_or(|hi| value.is_some_and(|v| v <= hi))
}

/// Whether `want`, when given and not empty, holds what `has` says.
fn any_of<T>(want: Option<&Vec<T>>, has: impl Fn(&T) -> bool) -> bool {
    want.is_none_or(|want| want.is_empty() || want.iter().any(has))
}

/// Test a media item against the smart-list filter predicates. `score_*` is
/// matched against the provider rating (a 0–10 vote average); `status`,
/// `user_score_*` and `tags` against the user's library, so an untracked
/// title never matches one of them. `tagged` holds the public ids of the
/// titles whose entry carries any of the filter's tags.
pub(super) fn matches_filter(
    media: &MediaWithUserState,
    filter: &SmartFilter,
    tagged: &HashSet<String>,
) -> bool {
    let genre_ok = any_of(filter.genres.as_ref(), |want| {
        media.genres.iter().any(|g| g.eq_ignore_ascii_case(want))
    });
    let year_ok = within(media.year, filter.year_min, filter.year_max);
    let score_ok = within(
        media.rating,
        filter.score_min.map(f64::from),
        filter.score_max.map(f64::from),
    );
    let status_ok = any_of(filter.statuses.as_ref(), |s| media.watch_status == Some(*s));
    let category_ok = any_of(filter.categories.as_ref(), |c| media.category == *c);
    // A tracked title's score is the user's; an untracked one has none.
    let own = media.watch_status.and(media.score);
    let user_score_ok = within(own, filter.user_score_min, filter.user_score_max);
    let tag_ok = !filter.has_tags() || tagged.contains(&media.cameo_media_id);
    genre_ok && year_ok && score_ok && status_ok && category_ok && user_score_ok && tag_ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::enums::Category;

    #[test]
    fn filters_are_checked() {
        let ok = |v: serde_json::Value| parse_filter(&v).is_ok();
        assert!(ok(serde_json::json!({})));
        assert!(ok(
            serde_json::json!({"media_type": ["movie", "tv"], "status": ["watching"], "sort": "trending", "score_min": 5, "score_max": 10, "year_min": 1990, "year_max": 1990})
        ));
        assert!(ok(
            serde_json::json!({"sort": "library", "category": ["k-drama", "anime"], "tag": [Uuid::nil()], "user_score_min": 0, "user_score_max": 100})
        ));
        assert!(!ok(serde_json::json!({"media_type": ["book"]})));
        assert!(!ok(serde_json::json!({"status": ["binged"]})));
        assert!(!ok(serde_json::json!({"sort": "random"})));
        assert!(!ok(serde_json::json!({"year_min": 2000, "year_max": 1990})));
        assert!(!ok(serde_json::json!({"score_min": 11})));
        assert!(!ok(serde_json::json!({"score_min": 8, "score_max": 2})));
        assert!(!ok(serde_json::json!({"genre": "drama"})));
        assert!(!ok(serde_json::json!({"category": ["kdrama"]})));
        assert!(!ok(serde_json::json!({"tag": ["comfort"]})));
        assert!(!ok(serde_json::json!({"user_score_max": 101})));
        assert!(!ok(serde_json::json!({"user_score_min": -1})));
        assert!(!ok(
            serde_json::json!({"user_score_min": 80, "user_score_max": 70})
        ));
    }

    #[test]
    fn filters_parse_into_the_evaluators_types() {
        let parse = |v: serde_json::Value| parse_filter(&v).unwrap();
        assert_eq!(parse(serde_json::json!({})), SmartFilter::default());
        assert_eq!(
            parse(serde_json::json!({"media_type": []})).kinds,
            vec![MediaKind::Movie, MediaKind::Show]
        );
        let f = parse(serde_json::json!({
            "media_type": ["tv", "show", "movie"],
            "status": ["watching", "completed"],
            "category": ["k-drama"],
            "sort": "score",
        }));
        assert_eq!(f.kinds, vec![MediaKind::Show, MediaKind::Movie]);
        assert_eq!(
            f.statuses,
            Some(vec![EntryStatus::Watching, EntryStatus::Completed])
        );
        assert_eq!(f.categories, Some(vec![Category::KDrama]));
        assert_eq!(f.source, Source::Feed(DiscoverCategory::TopRated));
        for (sort, source) in [
            ("popular", Source::Feed(DiscoverCategory::Popular)),
            ("top_rated", Source::Feed(DiscoverCategory::TopRated)),
            ("rating", Source::Feed(DiscoverCategory::TopRated)),
            ("trending", Source::Feed(DiscoverCategory::Trending)),
            ("library", Source::Library),
        ] {
            assert_eq!(parse(serde_json::json!({ "sort": sort })).source, source);
        }
    }

    #[test]
    fn filter_errors_name_every_accepted_value() {
        let detail = |v: serde_json::Value| match parse_filter(&v) {
            Err(ListError::InvalidFilter(why)) => why,
            other => panic!("expected an invalid filter, got {other:?}"),
        };
        assert_eq!(
            detail(serde_json::json!({"media_type": ["book"]})),
            "`media_type` holds `movie`, `show` or `tv`"
        );
        assert_eq!(
            detail(serde_json::json!({"sort": "random"})),
            "`sort` is `popular`, `top_rated`, `rating`, `score`, `trending` or `library`"
        );
        assert_eq!(
            detail(serde_json::json!({"user_score_min": 101})),
            "scores are 0 to 100"
        );
        assert_eq!(
            detail(serde_json::json!({"user_score_min": 9, "user_score_max": 8})),
            "the minimum score is above the maximum"
        );
    }

    fn title(category: Category) -> MediaWithUserState {
        MediaWithUserState {
            cameo_media_id: "tmdb:show:1".into(),
            title: "A".into(),
            original_title: None,
            romaji_title: None,
            kind: MediaKind::Show,
            category,
            year: Some(2020),
            overview: None,
            poster_url: None,
            backdrop_url: None,
            genres: vec!["Drama".into()],
            rating: Some(8.0),
            episode_count: None,
            watch_status: None,
            rewatching: false,
            score: None,
            is_favorite: false,
        }
    }

    #[test]
    fn library_criteria_match_only_what_the_user_tracks() {
        let none = HashSet::new();
        let filter = |v: serde_json::Value| parse_filter(&v).unwrap();
        let kdrama = title(Category::KDrama);
        let mut scored = kdrama.clone();
        scored.watch_status = Some(EntryStatus::Completed);
        scored.score = Some(85);

        let rated_80 = filter(serde_json::json!({"user_score_min": 80}));
        assert!(!matches_filter(&kdrama, &rated_80, &none), "untracked");
        assert!(matches_filter(&scored, &rated_80, &none));
        let at_most_80 = filter(serde_json::json!({"user_score_max": 80}));
        assert!(!matches_filter(&scored, &at_most_80, &none));
        let mut unscored = scored.clone();
        unscored.score = None;
        assert!(!matches_filter(&unscored, &rated_80, &none), "unscored");

        let kdramas = filter(serde_json::json!({"category": ["k-drama", "anime"]}));
        assert!(matches_filter(&kdrama, &kdramas, &none));
        assert!(!matches_filter(&title(Category::Series), &kdramas, &none));
        assert!(matches_filter(
            &title(Category::Series),
            &filter(serde_json::json!({"category": []})),
            &none
        ));

        let tagged = filter(serde_json::json!({"tag": [Uuid::nil()]}));
        assert!(!matches_filter(&scored, &tagged, &none));
        let carrying = HashSet::from([scored.cameo_media_id.clone()]);
        assert!(matches_filter(&scored, &tagged, &carrying));
        assert!(matches_filter(
            &scored,
            &filter(serde_json::json!({"tag": []})),
            &none
        ));
    }
}
