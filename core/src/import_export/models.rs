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
