//! Import runs: an uploaded file becomes an `imports` row (keeping the
//! original file), each parsed row an `import_items` row with its outcome,
//! and matched rows are applied to the library through [`LibraryService`] —
//! so every change an import makes is in the event log like any other.

use std::sync::Arc;

use sea_orm::ActiveValue::{NotSet, Set, Unchanged};
use std::collections::HashMap;

use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, Iterable, Order, QueryFilter,
    QueryOrder, QuerySelect, Select, TransactionTrait,
};
use uuid::Uuid;

use crate::db::now;
use crate::entity::enums::{
    ConflictStrategy, DatePrecision, EntryStatus, ImportOutcome, ImportSource, ImportStatus,
    JobType, RefProvider,
};
use crate::entity::{import, import_item, media};
use crate::import_export::error::ImportExportError;
use crate::import_export::import::{mal, tmdb};
use crate::import_export::models::{ImportEntry, ImportReport};
use crate::jobs::repo as jobs_repo;
use crate::library::{EntryPatch, LibraryService, RunDate, RunPatch, repo as library_repo};
use crate::media::id::{MediaId, MediaKind, Provider};
use crate::media::{MediaError, MediaService, MetadataErrorKind, MetadataProvider, catalog};
use crate::patch::Patch;

pub struct ImportPipeline {
    db: DatabaseConnection,
    metadata: Arc<dyn MetadataProvider>,
}

/// Parse an uploaded file into normalised rows.
pub fn parse(source: ImportSource, bytes: &[u8]) -> Result<Vec<ImportEntry>, ImportExportError> {
    match source {
        ImportSource::MalXml => mal::parse_mal_xml(bytes),
        ImportSource::MalJson => mal::parse_mal_json(bytes),
        ImportSource::TmdbJson => tmdb::parse_tmdb_json(bytes),
    }
}

/// Record an upload and queue it for processing, in one transaction.
///
/// The original file is kept (bounded by `max_bytes`, the configured cap) so
/// the import can be inspected or re-run later. A file its source's parser
/// rejects is refused here, with the parser's
/// [`ImportExportError::ParseError`], rather than queued to fail later: the
/// uploader is still there to be told.
pub async fn submit(
    db: &DatabaseConnection,
    user_id: Uuid,
    source: ImportSource,
    filename: Option<String>,
    bytes: Vec<u8>,
    strategy: ConflictStrategy,
    max_bytes: u64,
) -> Result<import::Model, ImportExportError> {
    if bytes.len() as u64 > max_bytes {
        return Err(ImportExportError::TooLarge {
            size: bytes.len() as u64,
            max: max_bytes,
        });
    }
    parse(source, &bytes)?;
    let txn = db.begin().await?;
    let row = import::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user_id),
        source: Set(source),
        status: Set(ImportStatus::Pending),
        conflict_strategy: Set(strategy),
        original_filename: Set(filename),
        original_size: Set(bytes.len() as i64),
        original_file: Set(Some(bytes)),
        total_count: Set(0),
        applied_count: Set(0),
        skipped_count: Set(0),
        unmatched_count: Set(0),
        failed_count: Set(0),
        error: Set(None),
        started_at: Set(None),
        completed_at: Set(None),
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(&txn)
    .await?;
    jobs_repo::enqueue_job(
        &txn,
        jobs_repo::build_job(
            JobType::Import,
            Some(serde_json::json!({ "import_id": row.id })),
            3,
        ),
    )
    .await
    .map_err(|e| ImportExportError::Internal(e.to_string()))?;
    txn.commit().await?;
    Ok(row)
}

/// Imports without their original file, which only the pipeline reads:
/// listing a user's imports must not load every upload they ever made.
fn without_file() -> Select<import::Entity> {
    import::Entity::find()
        .select_only()
        .columns(import::Column::iter().filter(|c| !matches!(c, import::Column::OriginalFile)))
        .column_as(Expr::cust("NULL::bytea"), "original_file")
}

/// A user's import, if it is theirs, without its original file.
pub async fn find_for_user(
    db: &DatabaseConnection,
    user_id: Uuid,
    import_id: Uuid,
) -> Result<Option<import::Model>, ImportExportError> {
    Ok(without_file()
        .filter(import::Column::Id.eq(import_id))
        .filter(import::Column::UserId.eq(user_id))
        .one(db)
        .await?)
}

/// One page of a user's imports, newest first (ids are UUIDv7, so
/// time-ordered), before `before` when given, without their original files.
/// At most `limit` rows; ask for one more than a page to learn whether
/// another follows.
pub async fn list_for_user(
    db: &DatabaseConnection,
    user_id: Uuid,
    before: Option<Uuid>,
    limit: u64,
) -> Result<Vec<import::Model>, ImportExportError> {
    let mut query = without_file()
        .filter(import::Column::UserId.eq(user_id))
        .order_by(import::Column::Id, Order::Desc);
    if let Some(before) = before {
        query = query.filter(import::Column::Id.lt(before));
    }
    Ok(query.limit(limit).all(db).await?)
}

/// One row of an import, by its row number.
pub async fn find_item(
    db: &DatabaseConnection,
    import_id: Uuid,
    row_number: i32,
) -> Result<Option<import_item::Model>, ImportExportError> {
    Ok(import_item::Entity::find()
        .filter(import_item::Column::ImportId.eq(import_id))
        .filter(import_item::Column::RowNumber.eq(row_number))
        .one(db)
        .await?)
}

/// The rows of an import in file order, each with the catalog row it was
/// matched to, if any.
pub async fn items_with_matches(
    db: &DatabaseConnection,
    import_id: Uuid,
) -> Result<Vec<(import_item::Model, Option<media::Model>)>, ImportExportError> {
    let rows = items(db, import_id).await?;
    let ids: Vec<Uuid> = rows.iter().filter_map(|r| r.matched_media_id).collect();
    let matched: HashMap<Uuid, media::Model> = if ids.is_empty() {
        HashMap::new()
    } else {
        media::Entity::find()
            .filter(media::Column::Id.is_in(ids))
            .all(db)
            .await?
            .into_iter()
            .map(|m| (m.id, m))
            .collect()
    };
    Ok(rows
        .into_iter()
        .map(|r| {
            let m = r.matched_media_id.and_then(|id| matched.get(&id).cloned());
            (r, m)
        })
        .collect())
}

/// The user an import belongs to.
pub async fn owner(
    db: &DatabaseConnection,
    import_id: Uuid,
) -> Result<Option<Uuid>, ImportExportError> {
    Ok(import::Entity::find_by_id(import_id)
        .one(db)
        .await?
        .map(|i| i.user_id))
}

/// The rows of an import in file order.
pub async fn items(
    db: &DatabaseConnection,
    import_id: Uuid,
) -> Result<Vec<import_item::Model>, ImportExportError> {
    Ok(import_item::Entity::find()
        .filter(import_item::Column::ImportId.eq(import_id))
        .order_by(import_item::Column::RowNumber, Order::Asc)
        .all(db)
        .await?)
}

enum Applied {
    Applied,
    Skipped,
}

impl ImportPipeline {
    pub fn new(db: DatabaseConnection, metadata: Arc<dyn MetadataProvider>) -> Self {
        Self { db, metadata }
    }

    /// Process a submitted import. Safe to re-run after a crash: rows that
    /// already have an outcome are not applied twice.
    pub async fn run(&self, import_id: Uuid) -> Result<ImportReport, ImportExportError> {
        let imp = import::Entity::find_by_id(import_id)
            .one(&self.db)
            .await?
            .ok_or_else(|| ImportExportError::Internal(format!("import {import_id} not found")))?;
        self.set_status(imp.id, ImportStatus::Running, None).await?;

        let bytes = imp.original_file.clone().unwrap_or_default();
        let entries = match parse(imp.source, &bytes) {
            Ok(e) => e,
            Err(e) => {
                // A file that does not parse will never parse: fail the import
                // for good rather than retrying.
                self.set_status(imp.id, ImportStatus::Failed, Some(e.to_string()))
                    .await?;
                return Ok(ImportReport::default());
            }
        };
        self.record_items(imp.id, &entries).await?;

        let library = LibraryService::new(self.db.clone());
        for item in items(&self.db, imp.id).await? {
            if item.outcome != ImportOutcome::Pending {
                continue;
            }
            let entry: ImportEntry = match serde_json::from_value(item.raw.clone()) {
                Ok(e) => e,
                Err(e) => {
                    self.set_outcome(&item, ImportOutcome::Failed, None, Some(e.to_string()))
                        .await?;
                    continue;
                }
            };
            let media = match self.resolve(&entry).await {
                Ok(Some(m)) => m,
                Ok(None) => {
                    self.set_outcome(&item, ImportOutcome::Unmatched, None, None)
                        .await?;
                    continue;
                }
                Err(e) => {
                    self.set_outcome(&item, ImportOutcome::Failed, None, Some(e))
                        .await?;
                    continue;
                }
            };
            let outcome = match self
                .apply(&library, imp.user_id, &media, &entry, imp.conflict_strategy)
                .await
            {
                Ok(Applied::Applied) => (ImportOutcome::Applied, None),
                Ok(Applied::Skipped) => (ImportOutcome::Skipped, None),
                Err(e) => (ImportOutcome::Failed, Some(e)),
            };
            self.set_outcome(&item, outcome.0, Some(media.id), outcome.1)
                .await?;
        }

        let report = self.finish(imp.id).await?;
        Ok(report)
    }

    async fn record_items(
        &self,
        import_id: Uuid,
        entries: &[ImportEntry],
    ) -> Result<(), ImportExportError> {
        let txn = self.db.begin().await?;
        for (i, entry) in entries.iter().enumerate() {
            import_item::Entity::insert(import_item::ActiveModel {
                id: Set(Uuid::now_v7()),
                import_id: Set(import_id),
                row_number: Set(i as i32 + 1),
                raw: Set(serde_json::to_value(entry)
                    .map_err(|e| ImportExportError::Serialization(e.to_string()))?),
                outcome: Set(ImportOutcome::Pending),
                matched_media_id: Set(None),
                confidence: Set(None),
                candidates: Set(None),
                error: Set(None),
                created_at: NotSet,
                updated_at: NotSet,
            })
            .on_conflict_do_nothing_on([
                import_item::Column::ImportId,
                import_item::Column::RowNumber,
            ])
            .exec(&txn)
            .await?;
        }
        txn.commit().await?;
        Ok(())
    }

    /// Find the catalog row for an entry. `Ok(None)` is "unmatched": the row
    /// stays in `import_items` to be resolved by hand later.
    ///
    /// TMDB ids resolve directly (the kind comes from the source hint, else a
    /// movie-then-show probe). MAL and other ids resolve only when a
    /// `media_refs` row already links them.
    async fn resolve(&self, entry: &ImportEntry) -> Result<Option<media::Model>, String> {
        let ids = &entry.external_ids;
        for (provider, raw) in [
            (RefProvider::Mal, ids.mal_id.as_deref()),
            (RefProvider::Imdb, ids.imdb_id.as_deref()),
            (RefProvider::Tvdb, ids.tvdb_id.as_deref()),
        ] {
            if let Some(raw) = raw
                && let Some(found) = catalog::find_by_external(&self.db, provider, raw)
                    .await
                    .map_err(|e| e.to_string())?
            {
                return Ok(Some(found));
            }
        }
        let Some(tmdb_id) = ids.tmdb_id.as_deref().and_then(|s| s.parse::<i32>().ok()) else {
            return Ok(None);
        };
        let kind = match entry.source.as_str() {
            "tmdb:movie" => Some(MediaKind::Movie),
            "tmdb:tv" | "tmdb:show" => Some(MediaKind::Show),
            _ => None,
        };
        let kind = match kind {
            Some(k) => k,
            None => match self.probe_tmdb_kind(tmdb_id).await? {
                Some(k) => k,
                None => return Ok(None),
            },
        };
        let id = MediaId::new(Provider::Tmdb, kind, tmdb_id);
        match MediaService::new(self.db.clone(), self.metadata.clone())
            .ensure_in_catalog(&id)
            .await
        {
            Ok(m) => Ok(Some(m)),
            Err(MediaError::NotFound(_)) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Whether a bare TMDB id is a movie or a show: movie first, then show.
    /// `Ok(None)` when TMDB has neither; any other failure is the row's error
    /// rather than a false "unmatched".
    async fn probe_tmdb_kind(&self, tmdb_id: i32) -> Result<Option<MediaKind>, String> {
        for kind in [MediaKind::Movie, MediaKind::Show] {
            let id = MediaId::new(Provider::Tmdb, kind, tmdb_id);
            match self.metadata.details(&id).await {
                Ok(_) => return Ok(Some(kind)),
                Err(e) if e.kind == MetadataErrorKind::NotFound => continue,
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(None)
    }

    /// Apply one row under the conflict strategy. Never clears anything: a
    /// field the row does not carry is left as stored.
    async fn apply(
        &self,
        library: &LibraryService,
        user_id: Uuid,
        media: &media::Model,
        entry: &ImportEntry,
        strategy: ConflictStrategy,
    ) -> Result<Applied, String> {
        let existing = library_repo::find_entry(&self.db, user_id, media.id)
            .await
            .map_err(|e| e.to_string())?
            .filter(|e| e.deleted_at.is_none());
        if existing.is_some() && strategy == ConflictStrategy::Skip {
            return Ok(Applied::Skipped);
        }
        let status = entry.status.as_deref().and_then(EntryStatus::parse);
        let score = entry
            .score
            .filter(|s| (0..=100).contains(s))
            .map(|s| s as i16);
        let patch = match (&existing, strategy) {
            // Merge: only fill what the library lacks.
            (Some(cur), ConflictStrategy::Merge) => EntryPatch {
                score: match (cur.score, score) {
                    (None, Some(s)) => Patch::Value(s),
                    _ => Patch::Absent,
                },
                ..Default::default()
            },
            _ => EntryPatch {
                status: Some(status.unwrap_or(EntryStatus::Planning)),
                score: score.map_or(Patch::Absent, Patch::Value),
                ..Default::default()
            },
        };
        let created = existing.is_none();
        library
            .upsert(user_id, media.id, patch)
            .await
            .map_err(|e| e.to_string())?;

        // Dates go on the first run, and only when this import created the
        // entry or was asked to overwrite.
        if created || strategy == ConflictStrategy::Overwrite {
            let day = |d: chrono::DateTime<chrono::Utc>| RunDate {
                date: d.date_naive(),
                precision: DatePrecision::Day,
            };
            let patch = RunPatch {
                started: entry
                    .started_at
                    .map_or(Patch::Absent, |d| Patch::Value(day(d))),
                completed: entry
                    .completed_at
                    .map_or(Patch::Absent, |d| Patch::Value(day(d))),
            };
            if !(patch.started.is_absent() && patch.completed.is_absent())
                && let Some(run) = library
                    .runs(user_id, media.id)
                    .await
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .next()
            {
                library
                    .update_run(user_id, run.id, patch)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(Applied::Applied)
    }

    async fn set_outcome(
        &self,
        item: &import_item::Model,
        outcome: ImportOutcome,
        media_id: Option<Uuid>,
        error: Option<String>,
    ) -> Result<(), ImportExportError> {
        import_item::ActiveModel {
            id: Unchanged(item.id),
            outcome: Set(outcome),
            matched_media_id: Set(media_id),
            confidence: Set(media_id.map(|_| 1.0)),
            error: Set(error),
            ..Default::default()
        }
        .update(&self.db)
        .await?;
        Ok(())
    }

    async fn set_status(
        &self,
        import_id: Uuid,
        status: ImportStatus,
        error: Option<String>,
    ) -> Result<(), ImportExportError> {
        let mut am = import::ActiveModel {
            id: Unchanged(import_id),
            status: Set(status),
            error: Set(error),
            ..Default::default()
        };
        match status {
            ImportStatus::Running => am.started_at = Set(Some(now())),
            ImportStatus::Completed | ImportStatus::Failed => am.completed_at = Set(Some(now())),
            ImportStatus::Pending => {}
        }
        am.update(&self.db).await?;
        Ok(())
    }

    /// Tally the item outcomes onto the import and mark it completed.
    async fn finish(&self, import_id: Uuid) -> Result<ImportReport, ImportExportError> {
        let rows = items(&self.db, import_id).await?;
        let count = |o: ImportOutcome| rows.iter().filter(|r| r.outcome == o).count() as u32;
        let report = ImportReport {
            total: rows.len() as u32,
            applied: count(ImportOutcome::Applied),
            skipped: count(ImportOutcome::Skipped),
            unmatched: count(ImportOutcome::Unmatched),
            failed: count(ImportOutcome::Failed),
        };
        import::ActiveModel {
            id: Unchanged(import_id),
            status: Set(ImportStatus::Completed),
            completed_at: Set(Some(now())),
            total_count: Set(report.total as i32),
            applied_count: Set(report.applied as i32),
            skipped_count: Set(report.skipped as i32),
            unmatched_count: Set(report.unmatched as i32),
            failed_count: Set(report.failed as i32),
            ..Default::default()
        }
        .update(&self.db)
        .await?;
        Ok(report)
    }
}
