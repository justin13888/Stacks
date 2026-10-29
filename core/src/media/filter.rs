use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A smart list's stored filter: the criteria a title must pass, and where
/// its candidates come from. A member left out does not narrow the list.
///
/// This is also the `filter` of a list in the backup format, so a change to
/// its members is a change to that format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, JsonSchema)]
pub struct FilterQuery {
    /// Only these kinds: `movie`, `show` (`tv` reads as `show`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<Vec<String>>,
    /// Titles with any of these genres, compared case-insensitively.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub genre: Option<Vec<String>>,
    /// Titles in any of these browsing categories (`movie`, `series`,
    /// `anime`, `k-drama`, `j-drama`, `c-drama`, `tw-drama`, `th-drama`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<Vec<String>>,
    /// Released in or after this year.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year_min: Option<i32>,
    /// Released in or before this year.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year_max: Option<i32>,
    /// Only titles in the user's library with one of these statuses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<Vec<String>>,
    /// Only titles whose library entry carries any of these tags, by the
    /// tag's id. A tag deleted since matches nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<Vec<Uuid>>,
    /// The provider's rating out of 10, at least.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_min: Option<i32>,
    /// The provider's rating out of 10, at most.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_max: Option<i32>,
    /// The user's own score (0–100), at least; an unscored title fails it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_score_min: Option<i32>,
    /// The user's own score (0–100), at most; an unscored title fails it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_score_max: Option<i32>,
    /// Where candidates come from: the `popular` (default), `top_rated`
    /// (`rating` and `score` read as it) or `trending` discovery feed, or
    /// `library`, the user's own library.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
}
