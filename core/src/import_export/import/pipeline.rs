use std::sync::Arc;

use cameo::CameoClient;
use cameo::unified::SearchProvider;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait};

use crate::entity::watch_status;
use crate::import_export::error::ImportExportError;
use crate::import_export::models::{
    ConflictStrategy, FailedEntry, ImportEntry, ImportReport,
};

pub struct ImportPipeline {
    pub db: DatabaseConnection,
    pub cameo: Arc<CameoClient>,
}

impl ImportPipeline {
    pub fn new(db: DatabaseConnection, cameo: Arc<CameoClient>) -> Self {
        Self { db, cameo }
    }

    /// Run a full import for a user given a list of pre-parsed `ImportEntry` values.
    pub async fn run(
        &self,
        user_id: &str,
        entries: Vec<ImportEntry>,
        conflict_strategy: ConflictStrategy,
    ) -> Result<ImportReport, ImportExportError> {
        let mut report = ImportReport {
            total: entries.len() as u32,
            ..Default::default()
        };

        for entry in entries {
            match self.process_entry(user_id, &entry, &conflict_strategy).await {
                Ok(EntryOutcome::Inserted) => report.succeeded += 1,
                Ok(EntryOutcome::Skipped) => report.skipped += 1,
                Err(reason) => {
                    report.failed += 1;
                    report.failed_entries.push(FailedEntry {
                        entry: entry.clone(),
                        reason,
                    });
                }
            }
        }

        Ok(report)
    }

    /// Resolve the canonical Cameo media ID for a single entry using its external IDs.
    pub async fn resolve_cameo_id(
        &self,
        entry: &ImportEntry,
    ) -> Result<Option<String>, ImportExportError> {
        if let Some(tmdb) = &entry.external_ids.tmdb_id {
            return Ok(Some(format!("tmdb:{tmdb}")));
        }
        // Fallback: search by title — best-effort, returns the first hit.
        if let Some(title) = entry_title(entry) {
            let res = self
                .cameo
                .search_movies(&title, Some(1))
                .await
                .map_err(|e| ImportExportError::ParseError(e.to_string()))?;
            if let Some(first) = res.results.first() {
                return Ok(Some(first.provider_id.clone()));
            }
        }
        Ok(None)
    }

    async fn process_entry(
        &self,
        user_id: &str,
        entry: &ImportEntry,
        strategy: &ConflictStrategy,
    ) -> Result<EntryOutcome, String> {
        let cameo_id = self
            .resolve_cameo_id(entry)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "could not resolve media".to_string())?;

        let existing = watch_status::Entity::find_by_id((user_id.to_string(), cameo_id.clone()))
            .one(&self.db)
            .await
            .map_err(|e| e.to_string())?;

        let now = Utc::now();
        match (existing, strategy) {
            (Some(_), ConflictStrategy::Skip) => Ok(EntryOutcome::Skipped),
            (Some(prev), strat) => {
                let mut active: watch_status::ActiveModel = prev.clone().into();
                let merge = matches!(strat, ConflictStrategy::Merge);
                if let Some(s) = entry.status.as_deref() {
                    if !merge || prev.status.is_empty() {
                        active.status = Set(s.to_string());
                    }
                }
                if let Some(score) = entry.score {
                    if !merge || prev.score.is_none() {
                        active.score = Set(Some(score));
                    }
                }
                if let Some(c) = entry.completed_at {
                    if !merge || prev.completed_at.is_none() {
                        active.completed_at = Set(Some(c.into()));
                    }
                }
                active.updated_at = Set(now.into());
                active.update(&self.db).await.map_err(|e| e.to_string())?;
                Ok(EntryOutcome::Inserted)
            }
            (None, _) => {
                let model = watch_status::ActiveModel {
                    user_id: Set(user_id.to_string()),
                    cameo_media_id: Set(cameo_id),
                    status: Set(entry
                        .status
                        .clone()
                        .unwrap_or_else(|| "completed".to_string())),
                    score: Set(entry.score),
                    score_date: Set(entry.completed_at.map(|t| t.into())),
                    started_at: Set(entry.started_at.map(|t| t.into())),
                    completed_at: Set(entry.completed_at.map(|t| t.into())),
                    repeat_count: Set(0),
                    notes: Set(None),
                    is_favorite: Set(false),
                    updated_at: Set(now.into()),
                };
                model.insert(&self.db).await.map_err(|e| e.to_string())?;
                Ok(EntryOutcome::Inserted)
            }
        }
    }
}

enum EntryOutcome {
    Inserted,
    Skipped,
}

fn entry_title(_entry: &ImportEntry) -> Option<String> {
    // ImportEntry today does not carry a title field. Sources that have one
    // should set tmdb_id directly. When we extend ImportEntry with a title
    // (e.g. for IMDb CSV) this hook can return it.
    None
}
