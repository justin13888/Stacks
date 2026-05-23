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

/// Normalise a third-party status string to the canonical Stacks vocabulary
/// (`planning | watching | completed | paused | dropped`). Unrecognised inputs
/// are returned lowercased and underscored, so the original is preserved.
pub fn normalize_status(s: &str) -> String {
    let s = s.trim().to_ascii_lowercase().replace([' ', '-'], "_");
    match s.as_str() {
        "completed" | "finished" => "completed".to_string(),
        "watching" | "currently_watching" | "current" | "in_progress" => "watching".to_string(),
        "plan_to_watch" | "planned" | "plan_to_read" | "planning" => "planning".to_string(),
        "paused" | "on_hold" | "hold" => "paused".to_string(),
        "dropped" => "dropped".to_string(),
        _ => s,
    }
}
