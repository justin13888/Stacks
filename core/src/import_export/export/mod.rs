pub mod csv;
pub mod json;

pub use csv::export_csv;
pub use json::export_json;

use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::import_export::error::ImportExportError;
use crate::import_export::models::{ExternalIds, ImportEntry};
use crate::library::LibraryService;
use crate::media::catalog;
use crate::media::id::Provider;

/// The user's live library as portable rows (status, score and the first
/// run's dates), in the shape the importers read back.
pub async fn library_rows(
    db: &DatabaseConnection,
    user_id: Uuid,
) -> Result<Vec<ImportEntry>, ImportExportError> {
    let library = LibraryService::new(db.clone());
    let views = library
        .all(user_id)
        .await
        .map_err(|e| ImportExportError::Internal(e.to_string()))?;
    let media_ids: Vec<Uuid> = views.iter().map(|v| v.media.id).collect();
    let refs = catalog::primary_refs(db, &media_ids).await?;

    let mut out = Vec::with_capacity(views.len());
    for v in views {
        let mut external_ids = ExternalIds::default();
        let mut source = String::new();
        if let Some(id) = refs.get(&v.media.id) {
            let numeric = Some(id.numeric.to_string());
            match id.provider {
                Provider::Tmdb => external_ids.tmdb_id = numeric,
                Provider::AniList => external_ids.anilist_id = numeric,
            }
            source = format!("{}:{}", id.provider, id.kind);
        }
        let runs = library
            .runs(user_id, v.media.id)
            .await
            .map_err(|e| ImportExportError::Internal(e.to_string()))?;
        let first = runs.first();
        let at_midnight = |d: chrono::NaiveDate| d.and_hms_opt(0, 0, 0).map(|t| t.and_utc());
        out.push(ImportEntry {
            external_ids,
            status: Some(v.entry.status.as_str().to_owned()),
            score: v.entry.score.map(i32::from),
            progress: v
                .progress
                .as_ref()
                .and_then(|p| i32::try_from(p.current_run_episodes).ok()),
            started_at: first.and_then(|r| r.started_on).and_then(at_midnight),
            completed_at: first.and_then(|r| r.completed_on).and_then(at_midnight),
            source,
        });
    }
    Ok(out)
}
