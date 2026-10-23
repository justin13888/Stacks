use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub use crate::entity::enums::ConflictStrategy;
use crate::entity::enums::EntryStatus;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExternalIds {
    pub mal_id: Option<String>,
    pub tmdb_id: Option<String>,
    pub imdb_id: Option<String>,
    pub anilist_id: Option<String>,
    pub tvdb_id: Option<String>,
}

/// One row of a third-party export, normalised. Stored verbatim as
/// `import_items.raw`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportEntry {
    pub external_ids: ExternalIds,
    /// A Stacks status token (see [`normalize_status`]), or `None`.
    pub status: Option<String>,
    pub score: Option<i32>,
    pub progress: Option<i32>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub source: String,
}

/// What one import run did.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub total: u32,
    pub applied: u32,
    pub skipped: u32,
    pub unmatched: u32,
    pub failed: u32,
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
/// `watching`. Unrecognised inputs are returned lowercased and underscored,
/// so the original is preserved and the row can be reported as unparseable.
pub fn normalize_status(s: &str) -> String {
    let s = s.trim().to_ascii_lowercase().replace([' ', '-'], "_");
    let status = match s.as_str() {
        "completed" | "finished" => EntryStatus::Completed,
        "watching" | "currently_watching" | "current" | "in_progress" | "rewatching"
        | "repeating" | "rereading" => EntryStatus::Watching,
        "plan_to_watch" | "planned" | "plan_to_read" | "planning" => EntryStatus::Planning,
        "paused" | "on_hold" | "hold" => EntryStatus::Paused,
        "dropped" => EntryStatus::Dropped,
        _ => return s,
    };
    status.as_str().to_owned()
}

#[cfg(test)]
mod tests {
    use super::{normalize_status, scale_ten_to_hundred};

    #[test]
    fn maps_common_external_statuses() {
        assert_eq!(normalize_status("Completed"), "completed");
        assert_eq!(normalize_status("Currently Watching"), "watching");
        assert_eq!(normalize_status("Plan to Watch"), "planning");
        assert_eq!(normalize_status("On-Hold"), "paused");
        assert_eq!(normalize_status("Dropped"), "dropped");
        assert_eq!(normalize_status("Repeating"), "watching");
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
}
