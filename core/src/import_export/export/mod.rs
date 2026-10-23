pub mod csv;
pub mod json;

pub use csv::export_csv;
pub use json::export_json;

use chrono::NaiveDate;
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::entity::enums::EntryStatus;
use crate::import_export::error::ImportExportError;
use crate::import_export::models::{ExternalIds, ImportEntry};
use crate::library::LibraryService;
use crate::media::catalog;
use crate::media::id::{MediaId, Provider};

/// One live library entry as an export carries it: the title's public
/// reference, its status and score, the current run's progress and the first
/// run's dates.
///
/// This is what the exports hold today, not the lossless format: rewatches,
/// individual plays, notes, tags and lists are not in it.
#[derive(Debug, Clone, PartialEq)]
pub struct BackupRow {
    /// `None` only for a catalog row without a primary reference, which the
    /// catalog's invariants rule out for any row Stacks wrote.
    pub media_id: Option<MediaId>,
    pub status: EntryStatus,
    /// 0-100.
    pub score: Option<i16>,
    /// Episodes watched in the current run.
    pub progress: Option<i64>,
    pub started_on: Option<NaiveDate>,
    pub completed_on: Option<NaiveDate>,
}

/// The user's live library, entry by entry, most recently updated first.
pub async fn backup_rows(
    db: &DatabaseConnection,
    user_id: Uuid,
) -> Result<Vec<BackupRow>, ImportExportError> {
    let library = LibraryService::new(db.clone());
    let views = library
        .all(user_id)
        .await
        .map_err(|e| ImportExportError::Internal(e.to_string()))?;
    let media_ids: Vec<Uuid> = views.iter().map(|v| v.media.id).collect();
    let refs = catalog::primary_refs(db, &media_ids).await?;

    let mut out = Vec::with_capacity(views.len());
    for v in views {
        let runs = library
            .runs(user_id, v.media.id)
            .await
            .map_err(|e| ImportExportError::Internal(e.to_string()))?;
        let first = runs.first();
        out.push(BackupRow {
            media_id: refs.get(&v.media.id).cloned(),
            status: v.entry.status,
            score: v.entry.score,
            progress: v.progress.as_ref().map(|p| p.current_run_episodes),
            started_on: first.and_then(|r| r.started_on),
            completed_on: first.and_then(|r| r.completed_on),
        });
    }
    Ok(out)
}

/// The user's live library as portable rows (status, score and the first
/// run's dates), in the shape the importers read back.
pub async fn library_rows(
    db: &DatabaseConnection,
    user_id: Uuid,
) -> Result<Vec<ImportEntry>, ImportExportError> {
    Ok(backup_rows(db, user_id)
        .await?
        .into_iter()
        .map(ImportEntry::from)
        .collect())
}

impl From<BackupRow> for ImportEntry {
    fn from(row: BackupRow) -> Self {
        let mut external_ids = ExternalIds::default();
        let mut source = String::new();
        if let Some(id) = &row.media_id {
            let numeric = Some(id.numeric.to_string());
            match id.provider {
                Provider::Tmdb => external_ids.tmdb_id = numeric,
                Provider::AniList => external_ids.anilist_id = numeric,
            }
            source = format!("{}:{}", id.provider, id.kind);
        }
        let at_midnight = |d: NaiveDate| d.and_hms_opt(0, 0, 0).map(|t| t.and_utc());
        ImportEntry {
            external_ids,
            status: Some(row.status.as_str().to_owned()),
            score: row.score.map(i32::from),
            progress: row.progress.and_then(|p| i32::try_from(p).ok()),
            started_at: row.started_on.and_then(at_midnight),
            completed_at: row.completed_on.and_then(at_midnight),
            source,
        }
    }
}
