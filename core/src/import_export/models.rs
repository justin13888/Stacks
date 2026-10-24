use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

pub use crate::entity::enums::ConflictStrategy;
use crate::entity::enums::{DatePrecision, EntryStatus, MediaKind};

/// The ids a third-party row names its title by.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ExternalIds {
    pub mal_id: Option<String>,
    pub tmdb_id: Option<String>,
    pub imdb_id: Option<String>,
    pub anilist_id: Option<String>,
    pub tvdb_id: Option<String>,
}

/// A date a source gave, and how much of it it gave.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportDate {
    /// The first day of what is known (a month-precision date is its 1st).
    pub date: NaiveDate,
    pub precision: DatePrecision,
}

/// One row of a third-party export, normalised. Stored as
/// `import_items.raw`, so the import job (and resolving the row by hand
/// later) reads exactly what the parser read; the file itself is kept on the
/// `imports` row.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ImportEntry {
    /// Which importer read the row: `mal` or `tmdb`.
    pub source: String,
    pub external_ids: ExternalIds,
    /// The title as the source names it, for matching by title.
    #[serde(default)]
    pub title: Option<String>,
    /// Movie or show, when the source says.
    #[serde(default)]
    pub kind: Option<MediaKind>,
    /// The title's release year, when the source gives it.
    #[serde(default)]
    pub year: Option<i32>,
    /// The title's episode count, when the source gives it.
    #[serde(default)]
    pub total_episodes: Option<i32>,
    /// A Stacks status token (see [`normalize_status`]), or `None`.
    pub status: Option<String>,
    /// 0–100; `None` when the source has no score (MAL writes 0 for that).
    pub score: Option<i32>,
    /// Episodes watched (MAL), or 1 for a watched movie.
    pub progress: Option<i32>,
    /// Completed rewatches after the first watch.
    #[serde(default)]
    pub rewatch_count: Option<i32>,
    /// Whether the user is on a rewatch now.
    #[serde(default)]
    pub rewatching: bool,
    #[serde(default)]
    pub started: Option<ImportDate>,
    #[serde(default)]
    pub completed: Option<ImportDate>,
    /// The user's comments on the title.
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub favourite: Option<bool>,
}

/// What one import run did. `total` is always the sum of the others: no
/// row is ever dropped.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub total: u32,
    pub applied: u32,
    pub skipped: u32,
    pub unmatched: u32,
    pub failed: u32,
    /// Rows waiting for the metadata provider; the job retries them.
    pub pending_retry: u32,
}

/// Convert a 0-10 provider rating to the 0-100 score Stacks stores.
///
/// Both MyAnimeList and TMDB rate out of 10, while `library_entries.score` is
/// 0-100. Storing the raw value made an excellent 9/10 import as 9/100 — near
/// the bottom of the scale rather than near the top.
pub fn scale_ten_to_hundred(rating: f64) -> i32 {
    (rating.clamp(0.0, 10.0) * 10.0).round() as i32
}

/// Normalise a third-party status string to a Stacks status token
/// (`planning | watching | completed | paused | dropped`). A rewatch is
/// `watching`. MAL's numeric codes (1 watching, 2 completed, 3 on hold,
/// 4 dropped, 6 plan to watch) are read too. Unrecognised inputs are
/// returned lowercased and underscored, so the original is preserved and
/// the row can be reported as unparseable.
pub fn normalize_status(s: &str) -> String {
    let s = s.trim().to_ascii_lowercase().replace([' ', '-'], "_");
    let status = match s.as_str() {
        "completed" | "finished" | "2" => EntryStatus::Completed,
        "watching" | "currently_watching" | "current" | "in_progress" | "rewatching"
        | "repeating" | "rereading" | "1" => EntryStatus::Watching,
        "plan_to_watch" | "planned" | "plan_to_read" | "planning" | "watchlist" | "6" => {
            EntryStatus::Planning
        }
        "paused" | "on_hold" | "onhold" | "hold" | "3" => EntryStatus::Paused,
        "dropped" | "4" => EntryStatus::Dropped,
        _ => return s,
    };
    status.as_str().to_owned()
}

/// Parse a source date that may leave its month or day out: `2024-05-17`,
/// `2024-05`, `2024`, or MAL's zero-filled `2024-05-00`, `2024-00-00`.
/// MAL's `0000-00-00` (and anything unparseable) is no date at all.
pub fn parse_partial_date(raw: &str) -> Option<ImportDate> {
    let mut parts = raw.trim().splitn(3, '-');
    let year: i32 = parts.next()?.parse().ok()?;
    if year <= 0 {
        return None;
    }
    let month: u32 = parts.next().map_or(Some(0), |m| m.parse().ok())?;
    let day: u32 = parts.next().map_or(Some(0), |d| d.parse().ok())?;
    let (month, day, precision) = match (month, day) {
        (0, _) => (1, 1, DatePrecision::Year),
        (m, 0) => (m, 1, DatePrecision::Month),
        (m, d) => (m, d, DatePrecision::Day),
    };
    Some(ImportDate {
        date: NaiveDate::from_ymd_opt(year, month, day)?,
        precision,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_common_external_statuses() {
        assert_eq!(normalize_status("Completed"), "completed");
        assert_eq!(normalize_status("Currently Watching"), "watching");
        assert_eq!(normalize_status("Plan to Watch"), "planning");
        assert_eq!(normalize_status("On-Hold"), "paused");
        assert_eq!(normalize_status("Dropped"), "dropped");
        assert_eq!(normalize_status("Repeating"), "watching");
        assert_eq!(normalize_status("6"), "planning");
        assert_eq!(normalize_status("3"), "paused");
    }

    #[test]
    fn ratings_scale_from_ten_to_a_hundred() {
        assert_eq!(scale_ten_to_hundred(9.0), 90);
        assert_eq!(scale_ten_to_hundred(10.0), 100);
        assert_eq!(scale_ten_to_hundred(0.0), 0);
        assert_eq!(scale_ten_to_hundred(7.5), 75);
        // Out-of-range input is clamped rather than producing an invalid score.
        assert_eq!(scale_ten_to_hundred(11.0), 100);
        assert_eq!(scale_ten_to_hundred(-1.0), 0);
    }

    #[test]
    fn passes_unknown_through_lowercased() {
        assert_eq!(normalize_status("UNRECOGNISED"), "unrecognised");
    }

    #[test]
    fn partial_dates_keep_their_precision() {
        let d = |y, m, day| NaiveDate::from_ymd_opt(y, m, day).unwrap();
        assert_eq!(
            parse_partial_date("2024-05-17"),
            Some(ImportDate {
                date: d(2024, 5, 17),
                precision: DatePrecision::Day
            })
        );
        for raw in ["2024-05-00", "2024-05"] {
            assert_eq!(
                parse_partial_date(raw),
                Some(ImportDate {
                    date: d(2024, 5, 1),
                    precision: DatePrecision::Month
                }),
                "{raw}"
            );
        }
        for raw in ["2024-00-00", "2024"] {
            assert_eq!(
                parse_partial_date(raw),
                Some(ImportDate {
                    date: d(2024, 1, 1),
                    precision: DatePrecision::Year
                }),
                "{raw}"
            );
        }
        for raw in ["0000-00-00", "", "soon", "2024-13-01", "2024-02-30"] {
            assert_eq!(parse_partial_date(raw), None, "{raw}");
        }
    }
}
