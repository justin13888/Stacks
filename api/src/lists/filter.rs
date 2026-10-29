//! A smart list's filter as clients write and read it, and its stored form.

use kynos::prelude::*;
use serde::{Deserialize, Serialize};
use stacks_core::entity::enums::{Category as CoreCategory, EntryStatus};
use stacks_core::media::filter::FilterQuery;
use uuid::Uuid;

use crate::catalog::{Category, Kind, Status};
use crate::problem::Internal;

/// Where a smart list draws its candidates from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartSort {
    /// Popular on TMDB (the default).
    Popular,
    /// Highest rated on TMDB.
    TopRated,
    /// Trending on TMDB this week.
    Trending,
    /// The caller's own library: every title they track, most recently
    /// changed first, read without calling a provider.
    Library,
}

impl SmartSort {
    fn as_str(self) -> &'static str {
        match self {
            Self::Popular => "popular",
            Self::TopRated => "top_rated",
            Self::Trending => "trending",
            Self::Library => "library",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "popular" => Some(Self::Popular),
            "top_rated" | "rating" | "score" => Some(Self::TopRated),
            "trending" => Some(Self::Trending),
            "library" => Some(Self::Library),
            _ => None,
        }
    }
}

/// What a smart list holds: the titles of its source that pass every
/// criterion given. A criterion left out does not narrow the list.
#[derive(Clone, Debug, Default, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct ListFilter {
    /// Only these kinds; both when absent.
    pub kinds: Option<Vec<Kind>>,
    /// Titles with any of these genres (case-insensitive).
    pub genres: Option<Vec<String>>,
    /// Titles in any of these browsing categories.
    pub categories: Option<Vec<Category>>,
    /// Released in or after this year.
    pub year_min: Option<i32>,
    /// Released in or before this year.
    pub year_max: Option<i32>,
    /// Only titles in the caller's library with one of these statuses.
    pub statuses: Option<Vec<Status>>,
    /// Only titles in the caller's library carrying any of these tags, by
    /// id. Each must be the caller's when it is added; one deleted since
    /// stays, and matches nothing.
    pub tags: Option<Vec<Uuid>>,
    /// The provider's rating out of 10, at least.
    #[schema(minimum = 0, maximum = 10)]
    pub rating_min: Option<i32>,
    /// The provider's rating out of 10, at most.
    #[schema(minimum = 0, maximum = 10)]
    pub rating_max: Option<i32>,
    /// The caller's own score (0–100), at least; a title they have not
    /// scored fails it.
    #[schema(minimum = 0, maximum = 100)]
    pub score_min: Option<i32>,
    /// The caller's own score (0–100), at most; a title they have not
    /// scored fails it.
    #[schema(minimum = 0, maximum = 100)]
    pub score_max: Option<i32>,
    /// Where candidates come from.
    pub sort: Option<SmartSort>,
}

impl ListFilter {
    /// The stored form, as core reads it.
    pub(super) fn into_stored(self) -> Result<serde_json::Value, Internal> {
        let query = FilterQuery {
            media_type: self.kinds.map(|ks| {
                ks.into_iter()
                    .map(|k| match k {
                        Kind::Movie => "movie".to_owned(),
                        Kind::Show => "show".to_owned(),
                    })
                    .collect()
            }),
            genre: self.genres,
            category: self.categories.map(|cs| {
                cs.into_iter()
                    .map(|c| CoreCategory::from(c).as_str().to_owned())
                    .collect()
            }),
            year_min: self.year_min,
            year_max: self.year_max,
            status: self.statuses.map(|ss| {
                ss.into_iter()
                    .map(|s| EntryStatus::from(s).as_str().to_owned())
                    .collect()
            }),
            tag: self.tags,
            score_min: self.rating_min,
            score_max: self.rating_max,
            user_score_min: self.score_min,
            user_score_max: self.score_max,
            sort: self.sort.map(|s| s.as_str().to_owned()),
        };
        serde_json::to_value(query).map_err(Internal::log)
    }

    /// The stored form, read back. A value this release does not know is
    /// left out rather than failing the read.
    pub(super) fn from_stored(raw: &serde_json::Value) -> Self {
        let query: FilterQuery = serde_json::from_value(raw.clone()).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "unreadable smart-list filter; showing it empty");
            FilterQuery::default()
        });
        Self {
            kinds: query.media_type.map(|ts| {
                ts.iter()
                    .filter_map(|t| match t.as_str() {
                        "movie" => Some(Kind::Movie),
                        "tv" | "show" => Some(Kind::Show),
                        _ => None,
                    })
                    .collect()
            }),
            genres: query.genre,
            categories: query.category.map(|cs| {
                cs.iter()
                    .filter_map(|c| CoreCategory::parse(c).map(Category::from))
                    .collect()
            }),
            year_min: query.year_min,
            year_max: query.year_max,
            statuses: query.status.map(|ss| {
                ss.iter()
                    .filter_map(|s| EntryStatus::parse(s).map(Status::from))
                    .collect()
            }),
            tags: query.tag,
            rating_min: query.score_min,
            rating_max: query.score_max,
            score_min: query.user_score_min,
            score_max: query.user_score_max,
            sort: query.sort.as_deref().and_then(SmartSort::parse),
        }
    }
}
