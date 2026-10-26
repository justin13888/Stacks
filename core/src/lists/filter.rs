//! A smart list's filter: the stored [`FilterQuery`] read once into the
//! types its evaluator uses, and the predicate a candidate must pass.

use crate::entity::enums::EntryStatus;
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
    pub year_min: Option<i32>,
    pub year_max: Option<i32>,
    pub statuses: Option<Vec<EntryStatus>>,
    /// The provider rating bounds, 0 to 10.
    pub score_min: Option<i32>,
    pub score_max: Option<i32>,
    /// The discovery feed candidates come from.
    pub feed: DiscoverCategory,
}

impl Default for SmartFilter {
    /// The empty filter: both kinds of the popular feed, unnarrowed.
    fn default() -> Self {
        Self {
            kinds: vec![MediaKind::Movie, MediaKind::Show],
            genres: None,
            year_min: None,
            year_max: None,
            statuses: None,
            score_min: None,
            score_max: None,
            feed: DiscoverCategory::Popular,
        }
    }
}

/// The feed a stored `sort` names.
fn sort_feed(sort: &str) -> Option<DiscoverCategory> {
    match sort {
        "popular" => Some(DiscoverCategory::Popular),
        "top_rated" | "rating" | "score" => Some(DiscoverCategory::TopRated),
        "trending" => Some(DiscoverCategory::Trending),
        _ => None,
    }
}

/// A stored filter, read and checked into a [`SmartFilter`].
pub fn parse_filter(raw: &serde_json::Value) -> Result<SmartFilter, ListError> {
    let filter: FilterQuery =
        serde_json::from_value(raw.clone()).map_err(|e| ListError::InvalidFilter(e.to_string()))?;
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
    let feed = match filter.sort.as_deref() {
        None => DiscoverCategory::Popular,
        Some(sort) => sort_feed(sort).ok_or_else(|| {
            invalid("`sort` is `popular`, `top_rated`, `rating`, `score` or `trending`")
        })?,
    };
    if let (Some(lo), Some(hi)) = (filter.year_min, filter.year_max)
        && lo > hi
    {
        return Err(invalid("`year_min` is after `year_max`"));
    }
    for score in [filter.score_min, filter.score_max].into_iter().flatten() {
        if !(0..=10).contains(&score) {
            return Err(invalid("ratings are 0 to 10"));
        }
    }
    if let (Some(lo), Some(hi)) = (filter.score_min, filter.score_max)
        && lo > hi
    {
        return Err(invalid("the minimum rating is above the maximum"));
    }
    Ok(SmartFilter {
        kinds,
        genres: filter.genre,
        year_min: filter.year_min,
        year_max: filter.year_max,
        statuses,
        score_min: filter.score_min,
        score_max: filter.score_max,
        feed,
    })
}

/// Test a media item against the smart-list filter predicates. `score_*` is
/// matched against the provider rating (a 0–10 vote average); `status`
/// against the user's library status, so an untracked title never matches a
/// status filter.
pub(super) fn matches_filter(media: &MediaWithUserState, filter: &SmartFilter) -> bool {
    let genre_ok = filter.genres.as_ref().is_none_or(|genres| {
        genres.is_empty()
            || genres
                .iter()
                .any(|want| media.genres.iter().any(|g| g.eq_ignore_ascii_case(want)))
    });
    let year_ok = filter
        .year_min
        .is_none_or(|min| media.year.is_some_and(|y| y >= min))
        && filter
            .year_max
            .is_none_or(|max| media.year.is_some_and(|y| y <= max));
    let score_ok = filter
        .score_min
        .is_none_or(|min| media.rating.is_some_and(|r| r >= f64::from(min)))
        && filter
            .score_max
            .is_none_or(|max| media.rating.is_some_and(|r| r <= f64::from(max)));
    let status_ok = filter.statuses.as_ref().is_none_or(|statuses| {
        statuses.is_empty()
            || media
                .watch_status
                .is_some_and(|have| statuses.contains(&have))
    });
    genre_ok && year_ok && score_ok && status_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_are_checked() {
        let ok = |v: serde_json::Value| parse_filter(&v).is_ok();
        assert!(ok(serde_json::json!({})));
        assert!(ok(
            serde_json::json!({"media_type": ["movie", "tv"], "status": ["watching"], "sort": "trending", "score_min": 5, "score_max": 10, "year_min": 1990, "year_max": 1990})
        ));
        assert!(!ok(serde_json::json!({"media_type": ["book"]})));
        assert!(!ok(serde_json::json!({"status": ["binged"]})));
        assert!(!ok(serde_json::json!({"sort": "random"})));
        assert!(!ok(serde_json::json!({"year_min": 2000, "year_max": 1990})));
        assert!(!ok(serde_json::json!({"score_min": 11})));
        assert!(!ok(serde_json::json!({"score_min": 8, "score_max": 2})));
        assert!(!ok(serde_json::json!({"genre": "drama"})));
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
            "sort": "score",
        }));
        assert_eq!(f.kinds, vec![MediaKind::Show, MediaKind::Movie]);
        assert_eq!(
            f.statuses,
            Some(vec![EntryStatus::Watching, EntryStatus::Completed])
        );
        assert_eq!(f.feed, DiscoverCategory::TopRated);
        for (sort, feed) in [
            ("popular", DiscoverCategory::Popular),
            ("top_rated", DiscoverCategory::TopRated),
            ("rating", DiscoverCategory::TopRated),
            ("trending", DiscoverCategory::Trending),
        ] {
            assert_eq!(parse(serde_json::json!({ "sort": sort })).feed, feed);
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
            "`sort` is `popular`, `top_rated`, `rating`, `score` or `trending`"
        );
    }
}
