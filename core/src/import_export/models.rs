use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExternalIds {
    pub mal_id: Option<String>,
    pub tmdb_id: Option<String>,
    pub imdb_id: Option<String>,
    pub anilist_id: Option<String>,
    pub tvdb_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportEntry {
    pub external_ids: ExternalIds,
    pub status: Option<String>,
    pub score: Option<i32>,
    pub progress: Option<i32>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedEntry {
    pub entry: ImportEntry,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImportReport {
    pub total: u32,
    pub succeeded: u32,
    pub skipped: u32,
    pub failed: u32,
    pub failed_entries: Vec<FailedEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConflictStrategy {
    /// Skip entries that already have data.
    #[default]
    Skip,
    /// Overwrite existing data with the imported values.
    Overwrite,
    /// Merge: keep existing non-null fields, fill in missing ones.
    Merge,
}

/// Convert a 0-10 provider rating to the 0-100 score Stacks stores.
///
/// Both MyAnimeList and TMDB rate out of 10, while `watch_status.score` is
/// 0-100. Storing the raw value made an excellent 9/10 import as 9/100 — near
/// the bottom of the scale rather than near the top.
pub fn scale_ten_to_hundred(rating: f64) -> i32 {
    (rating.clamp(0.0, 10.0) * 10.0).round() as i32
}

/// Normalise a third-party status string to the canonical Stacks vocabulary
/// (`plan_to_watch | watching | on_hold | completed | dropped | rewatching`).
/// Unrecognised inputs are returned lowercased and underscored, so the original
/// is preserved.
pub fn normalize_status(s: &str) -> String {
    let s = s.trim().to_ascii_lowercase().replace([' ', '-'], "_");
    match s.as_str() {
        "completed" | "finished" => "completed".to_string(),
        "watching" | "currently_watching" | "current" | "in_progress" => "watching".to_string(),
        "plan_to_watch" | "planned" | "plan_to_read" | "planning" => "plan_to_watch".to_string(),
        "paused" | "on_hold" | "hold" => "on_hold".to_string(),
        "dropped" => "dropped".to_string(),
        "rewatching" | "repeating" | "rereading" => "rewatching".to_string(),
        _ => s,
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_status, scale_ten_to_hundred};

    #[test]
    fn maps_common_external_statuses() {
        assert_eq!(normalize_status("Completed"), "completed");
        assert_eq!(normalize_status("Currently Watching"), "watching");
        assert_eq!(normalize_status("Plan to Watch"), "plan_to_watch");
        assert_eq!(normalize_status("On-Hold"), "on_hold");
        assert_eq!(normalize_status("Dropped"), "dropped");
        assert_eq!(normalize_status("Repeating"), "rewatching");
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
