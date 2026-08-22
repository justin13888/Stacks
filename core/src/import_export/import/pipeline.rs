use std::sync::Arc;

use crate::media::MediaClient;
use cameo::DetailProvider;
use chrono::{DateTime, FixedOffset, Utc};
use sea_orm::{ActiveValue::Set, DatabaseConnection};

use crate::entity::watch_status;
use crate::import_export::error::ImportExportError;
use crate::import_export::models::{ConflictStrategy, FailedEntry, ImportEntry, ImportReport};
use crate::media::id::{MediaId, MediaKind, Provider};
use crate::watch_state::repo as watch_repo;

pub struct ImportPipeline {
    pub db: DatabaseConnection,
    pub cameo: Arc<MediaClient>,
}

impl ImportPipeline {
    pub fn new(db: DatabaseConnection, cameo: Arc<MediaClient>) -> Self {
        Self { db, cameo }
    }

    /// Run a full import for a user. Each entry is resolved to a canonical id
    /// via [`resolve_cameo_id`](Self::resolve_cameo_id), then applied to the
    /// user's watch state under the chosen conflict strategy. Entries that
    /// cannot be resolved or that fail to apply are reported but do not abort
    /// the run.
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
            let cameo_id = match self.resolve_cameo_id(&entry).await? {
                Some(id) => id,
                None => {
                    report.failed += 1;
                    report.failed_entries.push(FailedEntry {
                        entry,
                        reason: "no canonical id could be resolved from external ids".to_string(),
                    });
                    continue;
                }
            };

            match self
                .apply_entry(user_id, &cameo_id, &entry, &conflict_strategy)
                .await
            {
                Ok(ApplyOutcome::Inserted | ApplyOutcome::Updated) => report.succeeded += 1,
                Ok(ApplyOutcome::Skipped) => report.skipped += 1,
                Err(e) => {
                    report.failed += 1;
                    report.failed_entries.push(FailedEntry {
                        entry,
                        reason: e.to_string(),
                    });
                }
            }
        }
        Ok(report)
    }

    /// Resolve the canonical Stacks media id for an entry from its external ids.
    ///
    /// TMDB ids are the only reliably-resolvable source today: when the entry
    /// source carries `tmdb:movie` / `tmdb:tv` the kind is known directly, and
    /// otherwise the pipeline probes `movie_details` first then falls back to
    /// `tv_show_details`. MAL/AniList/TVDB ids return `None` for now —
    /// cameo's unified API has no find-by-external-id endpoint.
    pub async fn resolve_cameo_id(
        &self,
        entry: &ImportEntry,
    ) -> Result<Option<String>, ImportExportError> {
        let Some(tmdb_raw) = entry.external_ids.tmdb_id.as_deref() else {
            // TODO(cameo): resolve MAL / AniList / IMDB / TVDB ids once cameo
            // exposes a find-by-external-id endpoint.
            return Ok(None);
        };
        let tmdb_id: i32 = match tmdb_raw.parse() {
            Ok(v) => v,
            Err(_) => return Ok(None),
        };

        // 1. If the source already encodes the kind, trust it (no cameo call).
        let hinted_kind = match entry.source.as_str() {
            "tmdb:movie" => Some(MediaKind::Movie),
            "tmdb:tv" => Some(MediaKind::Tv),
            _ => None,
        };
        if let Some(kind) = hinted_kind {
            return Ok(Some(
                MediaId::new(Provider::Tmdb, kind, tmdb_id).to_storage_string(),
            ));
        }

        // 2. Otherwise probe cameo: movies first, then TV.
        if self.cameo.movie_details(tmdb_id).await.is_ok() {
            return Ok(Some(
                MediaId::new(Provider::Tmdb, MediaKind::Movie, tmdb_id).to_storage_string(),
            ));
        }
        if self.cameo.tv_show_details(tmdb_id).await.is_ok() {
            return Ok(Some(
                MediaId::new(Provider::Tmdb, MediaKind::Tv, tmdb_id).to_storage_string(),
            ));
        }
        Ok(None)
    }

    /// Apply one entry to the user's watch state, honouring the conflict strategy.
    async fn apply_entry(
        &self,
        user_id: &str,
        cameo_media_id: &str,
        entry: &ImportEntry,
        strategy: &ConflictStrategy,
    ) -> Result<ApplyOutcome, ImportExportError> {
        let existing = watch_repo::find_status(&self.db, user_id, cameo_media_id)
            .await
            .map_err(|e| ImportExportError::Internal(e.to_string()))?;

        if existing.is_some() && matches!(strategy, ConflictStrategy::Skip) {
            return Ok(ApplyOutcome::Skipped);
        }

        let was_insert = existing.is_none();
        let merged = merge(existing.as_ref(), entry, strategy);
        let model = watch_status::ActiveModel {
            user_id: Set(user_id.to_string()),
            cameo_media_id: Set(cameo_media_id.to_string()),
            status: Set(merged.status),
            score: Set(merged.score),
            score_date: Set(merged.score_date),
            started_at: Set(merged.started_at),
            completed_at: Set(merged.completed_at),
            repeat_count: Set(merged.repeat_count),
            notes: Set(merged.notes),
            is_favorite: Set(merged.is_favorite),
            updated_at: Set(Utc::now().into()),
        };
        watch_repo::upsert_status(&self.db, model)
            .await
            .map_err(|e| ImportExportError::Internal(e.to_string()))?;
        Ok(if was_insert {
            ApplyOutcome::Inserted
        } else {
            ApplyOutcome::Updated
        })
    }
}

enum ApplyOutcome {
    Inserted,
    Updated,
    Skipped,
}

struct Merged {
    status: String,
    score: Option<i32>,
    score_date: Option<DateTime<FixedOffset>>,
    started_at: Option<DateTime<FixedOffset>>,
    completed_at: Option<DateTime<FixedOffset>>,
    repeat_count: i32,
    notes: Option<String>,
    is_favorite: bool,
}

/// Combine an existing watch-state row with a fresh import entry under the
/// chosen conflict strategy. `Skip` is handled by the caller before reaching
/// here, so this only resolves `Overwrite` and `Merge`.
fn merge(
    existing: Option<&watch_status::Model>,
    entry: &ImportEntry,
    strategy: &ConflictStrategy,
) -> Merged {
    let now: DateTime<FixedOffset> = Utc::now().into();
    let entry_started = entry.started_at.map(Into::into);
    let entry_completed = entry.completed_at.map(Into::into);
    let entry_status = entry
        .status
        .clone()
        .unwrap_or_else(|| "plan_to_watch".to_string());

    match (existing, strategy) {
        (Some(cur), ConflictStrategy::Merge) => Merged {
            status: cur.status.clone(),
            score: cur.score.or(entry.score),
            score_date: cur
                .score_date
                .or_else(|| entry.score.is_some().then_some(now)),
            started_at: cur.started_at.or(entry_started),
            completed_at: cur.completed_at.or(entry_completed),
            repeat_count: cur.repeat_count,
            notes: cur.notes.clone(),
            is_favorite: cur.is_favorite,
        },
        // Overwrite (or new insert): the entry wins.
        _ => Merged {
            status: entry_status,
            score: entry.score,
            score_date: entry.score.is_some().then_some(now),
            started_at: entry_started,
            completed_at: entry_completed,
            repeat_count: 0,
            notes: None,
            is_favorite: false,
        },
    }
}
