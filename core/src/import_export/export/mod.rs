pub mod csv;
pub mod json;

pub use csv::export_csv;
pub use json::export_json;

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::entity::watch_status;
use crate::import_export::error::ImportExportError;
use crate::import_export::models::{ExternalIds, ImportEntry};

/// Build the user's full export payload as a list of `ImportEntry` values.
/// Joins `watch_status` rows for the user; cameo metadata is not embedded
/// (only the IDs), so the export is round-trippable.
pub async fn build_user_export(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<Vec<ImportEntry>, ImportExportError> {
    let rows = watch_status::Entity::find()
        .filter(watch_status::Column::UserId.eq(user_id))
        .all(db)
        .await?;

    Ok(rows
        .into_iter()
        .map(|r| ImportEntry {
            external_ids: ExternalIds {
                tmdb_id: r
                    .cameo_media_id
                    .strip_prefix("tmdb:")
                    .map(|s| s.to_string()),
                ..Default::default()
            },
            status: Some(r.status),
            score: r.score,
            progress: None,
            started_at: r.started_at.map(|t| t.with_timezone(&chrono::Utc)),
            completed_at: r.completed_at.map(|t| t.with_timezone(&chrono::Utc)),
            source: "stacks".to_string(),
        })
        .collect())
}
