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

impl FilterQuery {
    /// Read a filter as it is stored, keeping what can be read.
    ///
    /// A row written before a member was typed may hold it in another
    /// shape (an earlier restore stored any object, so `"tag": ["comfort"]`
    /// or `"category": "anime"` can be on disk). Such a member was not
    /// defined when it was written, so it never narrowed the list; it is
    /// dropped, as the backup's version 1 upgrade drops it, and every
    /// member that reads on its own is kept. `None` when `raw` is not an
    /// object, which no writer stores.
    pub fn from_stored(raw: &serde_json::Value) -> Option<Self> {
        if let Ok(filter) = serde_json::from_value(raw.clone()) {
            return Some(filter);
        }
        let members = raw.as_object()?;
        let readable: serde_json::Map<_, _> = members
            .iter()
            .filter(|(key, value)| {
                let one = serde_json::json!({ key.as_str(): value });
                serde_json::from_value::<Self>(one).is_ok()
            })
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        tracing::warn!(
            dropped = members.len() - readable.len(),
            "a stored smart-list filter had unreadable members"
        );
        serde_json::from_value(serde_json::Value::Object(readable)).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_stored_filter_keeps_the_members_that_read() {
        let raw = json!({
            "genre": ["Drama"], "tag": ["comfort"], "category": "anime",
            "user_score_min": 80, "unknown": true,
        });
        assert_eq!(
            FilterQuery::from_stored(&raw),
            Some(FilterQuery {
                genre: Some(vec!["Drama".into()]),
                user_score_min: Some(80),
                ..Default::default()
            })
        );
        assert_eq!(FilterQuery::from_stored(&json!([1])), None);
    }
}
