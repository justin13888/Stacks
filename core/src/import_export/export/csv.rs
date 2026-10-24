//! The library as one flat CSV, for spreadsheets.
//!
//! One row per live library entry, with its progress and rewatch count
//! derived from its runs and plays. It is deliberately lossy — no individual
//! plays, no history, no lists — and Stacks does not import it: the JSON
//! backup ([`crate::import_export::backup`]) is the lossless format.

use std::collections::HashMap;

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::entity::{entry_tag, tag};
use crate::import_export::error::ImportExportError;
use crate::library::LibraryService;
use crate::library::repo as library_repo;
use crate::media::catalog;

/// The header, in column order.
pub const HEADER: &[&str] = &[
    "title",
    "name",
    "kind",
    "category",
    "status",
    "score",
    "favourite",
    "progress",
    "episodes",
    "plays",
    "rewatches",
    "started_on",
    "completed_on",
    "first_watched_at",
    "last_watched_at",
    "tags",
    "notes",
    "added_at",
    "updated_at",
];

/// The user's live library as CSV bytes, most recently updated first.
///
/// `progress` is the episodes watched in the current run, `plays` every
/// play that was not undone, `rewatches` the runs after the first, and the
/// dates are the current run's. Tags are joined with `; `.
pub async fn library_csv(
    db: &DatabaseConnection,
    user_id: Uuid,
) -> Result<Vec<u8>, ImportExportError> {
    let views = LibraryService::new(db.clone())
        .all(user_id)
        .await
        .map_err(|e| ImportExportError::Internal(e.to_string()))?;
    let entry_ids: Vec<Uuid> = views.iter().map(|v| v.entry.id).collect();
    let media_ids: Vec<Uuid> = views.iter().map(|v| v.media.id).collect();
    let refs = catalog::primary_refs(db, &media_ids).await?;
    let runs = library_repo::current_runs(db, &entry_ids).await?;
    let tags = tags_by_entry(db, user_id, &entry_ids).await?;

    let mut buf = Vec::new();
    {
        let mut out = csv::Writer::from_writer(&mut buf);
        out.write_record(HEADER)
            .map_err(|e| ImportExportError::Serialization(format!("CSV header: {e}")))?;
        for v in &views {
            let run = runs.get(&v.entry.id);
            let p = v.progress.as_ref();
            let opt = |x: Option<String>| x.unwrap_or_default();
            let record = [
                opt(refs.get(&v.media.id).map(ToString::to_string)),
                v.media.title.clone(),
                v.media.kind.to_string(),
                v.media.category.to_string(),
                v.entry.status.to_string(),
                opt(v.entry.score.map(|s| s.to_string())),
                v.entry.favourite.to_string(),
                opt(p.map(|p| p.current_run_episodes.to_string())),
                opt(v.media.episode_count.map(|n| n.to_string())),
                opt(p.map(|p| p.play_count.to_string())),
                opt(p.map(|p| p.rewatch_count.to_string())),
                opt(run.and_then(|r| r.started_on).map(|d| d.to_string())),
                opt(run.and_then(|r| r.completed_on).map(|d| d.to_string())),
                opt(p.and_then(|p| p.first_watched_at).map(|d| d.to_rfc3339())),
                opt(p.and_then(|p| p.last_watched_at).map(|d| d.to_rfc3339())),
                tags.get(&v.entry.id)
                    .map(|t| t.join("; "))
                    .unwrap_or_default(),
                opt(v.entry.notes.clone()),
                v.entry.created_at.to_rfc3339(),
                v.entry.updated_at.to_rfc3339(),
            ];
            out.write_record(&record)
                .map_err(|e| ImportExportError::Serialization(format!("CSV row: {e}")))?;
        }
        out.flush()
            .map_err(|e| ImportExportError::Io(format!("CSV flush: {e}")))?;
    }
    Ok(buf)
}

/// Each entry's tag names, alphabetically.
async fn tags_by_entry(
    db: &DatabaseConnection,
    user_id: Uuid,
    entry_ids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<String>>, ImportExportError> {
    let names: HashMap<Uuid, String> = tag::Entity::find()
        .filter(tag::Column::UserId.eq(user_id))
        .all(db)
        .await?
        .into_iter()
        .map(|t| (t.id, t.name))
        .collect();
    let mut out: HashMap<Uuid, Vec<String>> = HashMap::new();
    for chunk in entry_ids.chunks(crate::db::INSERT_CHUNK) {
        for a in entry_tag::Entity::find()
            .filter(entry_tag::Column::EntryId.is_in(chunk.to_vec()))
            .all(db)
            .await?
        {
            if let Some(name) = names.get(&a.tag_id) {
                out.entry(a.entry_id).or_default().push(name.clone());
            }
        }
    }
    for names in out.values_mut() {
        names.sort();
    }
    Ok(out)
}
