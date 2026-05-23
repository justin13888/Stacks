use crate::import_export::error::ImportExportError;
use crate::import_export::models::ImportEntry;

const HEADER: &[&str] = &[
    "source",
    "mal_id",
    "tmdb_id",
    "imdb_id",
    "anilist_id",
    "tvdb_id",
    "status",
    "score",
    "progress",
    "started_at",
    "completed_at",
];

/// Serialize a slice of [`ImportEntry`] values to CSV bytes.
pub fn export_csv(entries: &[ImportEntry]) -> Result<Vec<u8>, ImportExportError> {
    let mut buf = Vec::new();
    {
        let mut wtr = csv::Writer::from_writer(&mut buf);
        wtr.write_record(HEADER)
            .map_err(|e| ImportExportError::Serialization(format!("CSV header: {e}")))?;
        for entry in entries {
            wtr.write_record([
                entry.source.as_str(),
                entry.external_ids.mal_id.as_deref().unwrap_or(""),
                entry.external_ids.tmdb_id.as_deref().unwrap_or(""),
                entry.external_ids.imdb_id.as_deref().unwrap_or(""),
                entry.external_ids.anilist_id.as_deref().unwrap_or(""),
                entry.external_ids.tvdb_id.as_deref().unwrap_or(""),
                entry.status.as_deref().unwrap_or(""),
                &entry.score.map(|s| s.to_string()).unwrap_or_default(),
                &entry.progress.map(|p| p.to_string()).unwrap_or_default(),
                &entry.started_at.map(|d| d.to_rfc3339()).unwrap_or_default(),
                &entry
                    .completed_at
                    .map(|d| d.to_rfc3339())
                    .unwrap_or_default(),
            ])
            .map_err(|e| ImportExportError::Serialization(format!("CSV row: {e}")))?;
        }
        wtr.flush()
            .map_err(|e| ImportExportError::Io(format!("CSV flush: {e}")))?;
    }
    Ok(buf)
}
