use crate::import_export::error::ImportExportError;
use crate::import_export::models::ImportEntry;

/// Serialize a list of `ImportEntry` values to CSV bytes with a header row.
pub fn export_csv(entries: &[ImportEntry]) -> Result<Vec<u8>, ImportExportError> {
    let mut wtr = csv::WriterBuilder::new()
        .has_headers(true)
        .from_writer(Vec::new());
    wtr.write_record([
        "tmdb_id",
        "imdb_id",
        "mal_id",
        "anilist_id",
        "tvdb_id",
        "status",
        "score",
        "progress",
        "started_at",
        "completed_at",
        "source",
    ])
    .map_err(|e| ImportExportError::Serialization(e.to_string()))?;
    for e in entries {
        wtr.write_record([
            e.external_ids.tmdb_id.as_deref().unwrap_or(""),
            e.external_ids.imdb_id.as_deref().unwrap_or(""),
            e.external_ids.mal_id.as_deref().unwrap_or(""),
            e.external_ids.anilist_id.as_deref().unwrap_or(""),
            e.external_ids.tvdb_id.as_deref().unwrap_or(""),
            e.status.as_deref().unwrap_or(""),
            &e.score.map(|s| s.to_string()).unwrap_or_default(),
            &e.progress.map(|s| s.to_string()).unwrap_or_default(),
            &e.started_at.map(|t| t.to_rfc3339()).unwrap_or_default(),
            &e.completed_at.map(|t| t.to_rfc3339()).unwrap_or_default(),
            &e.source,
        ])
        .map_err(|e| ImportExportError::Serialization(e.to_string()))?;
    }
    wtr.into_inner()
        .map_err(|e| ImportExportError::Serialization(e.to_string()))
}
