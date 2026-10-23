//! Import runs: an uploaded file becomes an `imports` row (keeping the
//! original file), each parsed row an `import_items` row with its outcome,
//! and matched rows are applied to the library ([`super::apply`]) with their
//! `entry_events`, like any other library change.
//!
//! No row is ever dropped. Every row ends `applied`, `skipped`, `unmatched`
//! (with its candidates, to resolve by hand), `failed` (with the reason), or
//! — while the metadata provider is unavailable — `pending_retry`, which the
//! `import` job comes back to. An import's `total` is always the sum of the
//! five.

use std::collections::HashMap;
use std::sync::Arc;

use sea_orm::ActiveValue::{NotSet, Set, Unchanged};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, Iterable, Order, QueryFilter,
    QueryOrder, QuerySelect, Select, TransactionTrait,
};
use uuid::Uuid;

use crate::db::now;
use crate::entity::enums::{ConflictStrategy, ImportOutcome, ImportSource, ImportStatus, JobType};
use crate::entity::{import, import_item, media};
use crate::import_export::error::ImportExportError;
use crate::import_export::import::apply::{self, Applied};
use crate::import_export::import::matching::{Matcher, Resolution};
use crate::import_export::import::{mal, tmdb};
use crate::import_export::models::{ImportEntry, ImportReport};
use crate::jobs::repo as jobs_repo;
use crate::media::{MediaError, MediaId, MediaService, MetadataProvider};

/// How many times the `import` job runs before it gives up. Each run after
/// a metadata outage backs off (30 s doubling, capped at an hour), so this
/// keeps retrying a deferred import for about a day.
pub const IMPORT_JOB_ATTEMPTS: i32 = 30;

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
            IMPORT_JOB_ATTEMPTS,
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

/// The row an [`import_item`] holds, as the parser read it.
pub fn row_of(item: &import_item::Model) -> Result<ImportEntry, String> {
    serde_json::from_value(item.raw.clone()).map_err(|e| format!("unreadable import row: {e}"))
}

/// Why a row cannot be resolved by hand.
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    /// The import has no row with that number.
    #[error("the import has no row with that number")]
    NoSuchRow,
    /// The row was already applied or skipped, or has not been read yet.
    #[error("the row is {0}; only an unmatched, failed or retrying row can be resolved")]
    NotResolvable(ImportOutcome),
    /// The title named does not exist.
    #[error("{0}")]
    TitleNotFound(String),
    #[error(transparent)]
    Other(#[from] ImportExportError),
}

impl From<sea_orm::DbErr> for ResolveError {
    fn from(e: sea_orm::DbErr) -> Self {
        Self::Other(e.into())
    }
}

impl ImportPipeline {
    pub fn new(db: DatabaseConnection, metadata: Arc<dyn MetadataProvider>) -> Self {
        Self { db, metadata }
    }

    /// Process a submitted import: read the file into rows (once), then
    /// match and apply every row without an outcome yet, or waiting for a
    /// retry. Safe to re-run after a crash: a row with an outcome is never
    /// applied twice.
    ///
    /// When the metadata provider is unavailable, the rows it could not
    /// answer stay `pending_retry`, the import goes back to `pending`, and
    /// the report says how many are waiting: the job reschedules itself.
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

        let matcher = Matcher::new(self.db.clone(), self.metadata.clone());
        for item in items(&self.db, imp.id).await? {
            if !matches!(
                item.outcome,
                ImportOutcome::Pending | ImportOutcome::PendingRetry
            ) {
                continue;
            }
            let entry = match row_of(&item) {
                Ok(e) => e,
                Err(e) => {
                    self.set_outcome(&item, Outcome::failed(e)).await?;
                    continue;
                }
            };
            let outcome = match matcher.resolve(&entry).await {
                Resolution::Matched { media, confidence } => {
                    let applied = apply::apply(
                        &self.db,
                        imp.user_id,
                        &media,
                        &entry,
                        imp.conflict_strategy,
                        imp.id,
                    )
                    .await;
                    Outcome::applied(applied, media.id, confidence)
                }
                Resolution::Unmatched(candidates) => Outcome {
                    candidates: Some(
                        serde_json::to_value(candidates)
                            .map_err(|e| ImportExportError::Serialization(e.to_string()))?,
                    ),
                    ..Outcome::new(ImportOutcome::Unmatched)
                },
                Resolution::Retry(why) => Outcome {
                    error: Some(why),
                    ..Outcome::new(ImportOutcome::PendingRetry)
                },
                Resolution::Failed(why) => Outcome::failed(why),
            };
            self.set_outcome(&item, outcome).await?;
        }

        self.finish(imp.id).await
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

    /// Match one row of `import_id` to `title` by hand, and apply it under
    /// the import's conflict strategy. The row's MAL id is linked to the
    /// title, so the next import of it matches on its own.
    pub async fn resolve_item(
        &self,
        import_id: Uuid,
        row_number: i32,
        title: &MediaId,
    ) -> Result<(import_item::Model, media::Model), ResolveError> {
        let imp = import::Entity::find_by_id(import_id)
            .one(&self.db)
            .await?
            .ok_or(ResolveError::NoSuchRow)?;
        let item = find_item(&self.db, import_id, row_number)
            .await?
            .ok_or(ResolveError::NoSuchRow)?;
        if !matches!(
            item.outcome,
            ImportOutcome::Unmatched | ImportOutcome::Failed | ImportOutcome::PendingRetry
        ) {
            return Err(ResolveError::NotResolvable(item.outcome));
        }
        let entry = row_of(&item).map_err(ImportExportError::Internal)?;
        let media = match MediaService::new(self.db.clone(), self.metadata.clone())
            .ensure_in_catalog(title)
            .await
        {
            Ok(m) => m,
            Err(MediaError::NotFound(why)) => return Err(ResolveError::TitleNotFound(why)),
            Err(e) => return Err(ImportExportError::Internal(e.to_string()).into()),
        };
        if let Some(mal) = entry.external_ids.mal_id.as_deref() {
            crate::media::catalog::insert_extra_ref(
                &self.db,
                media.id,
                crate::entity::enums::RefProvider::Mal,
                media.kind,
                mal,
            )
            .await?;
        }
        let applied = apply::apply(
            &self.db,
            imp.user_id,
            &media,
            &entry,
            imp.conflict_strategy,
            imp.id,
        )
        .await
        .map_err(ImportExportError::Internal)?;
        let outcome = Outcome::applied(Ok(applied), media.id, 1.0);
        let item = self.set_outcome(&item, outcome).await?;
        self.tally(import_id).await?;
        Ok((item, media))
    }

    async fn set_outcome(
        &self,
        item: &import_item::Model,
        o: Outcome,
    ) -> Result<import_item::Model, ImportExportError> {
        let mut am = import_item::ActiveModel {
            id: Unchanged(item.id),
            outcome: Set(o.outcome),
            matched_media_id: Set(o.media_id),
            confidence: Set(o.confidence),
            error: Set(o.error),
            ..Default::default()
        };
        // A matched row keeps the candidates it was shown, if it was
        // resolved by hand from them.
        if !matches!(o.outcome, ImportOutcome::Applied | ImportOutcome::Skipped) {
            am.candidates = Set(o.candidates);
        }
        Ok(am.update(&self.db).await?)
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

    /// Count the rows' outcomes onto the import.
    async fn tally(&self, import_id: Uuid) -> Result<ImportReport, ImportExportError> {
        let rows = items(&self.db, import_id).await?;
        let count = |o: ImportOutcome| rows.iter().filter(|r| r.outcome == o).count() as u32;
        let report = ImportReport {
            total: rows.len() as u32,
            applied: count(ImportOutcome::Applied),
            skipped: count(ImportOutcome::Skipped),
            unmatched: count(ImportOutcome::Unmatched),
            failed: count(ImportOutcome::Failed),
            pending_retry: count(ImportOutcome::PendingRetry) + count(ImportOutcome::Pending),
        };
        import::ActiveModel {
            id: Unchanged(import_id),
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

    /// Count the outcomes, and mark the import completed — or, with rows
    /// still waiting for the metadata provider, pending again.
    async fn finish(&self, import_id: Uuid) -> Result<ImportReport, ImportExportError> {
        let report = self.tally(import_id).await?;
        if report.pending_retry > 0 {
            self.set_status(
                import_id,
                ImportStatus::Pending,
                Some(format!(
                    "{} rows are waiting for the metadata provider; the import retries them",
                    report.pending_retry
                )),
            )
            .await?;
        } else {
            self.set_status(import_id, ImportStatus::Completed, None)
                .await?;
        }
        Ok(report)
    }
}

/// What one row came to, as it is stored.
#[derive(Debug)]
struct Outcome {
    outcome: ImportOutcome,
    media_id: Option<Uuid>,
    confidence: Option<f32>,
    candidates: Option<serde_json::Value>,
    error: Option<String>,
}

impl Outcome {
    fn new(outcome: ImportOutcome) -> Self {
        Self {
            outcome,
            media_id: None,
            confidence: None,
            candidates: None,
            error: None,
        }
    }

    fn failed(why: String) -> Self {
        Self {
            error: Some(why),
            ..Self::new(ImportOutcome::Failed)
        }
    }

    fn applied(applied: Result<Applied, String>, media_id: Uuid, confidence: f32) -> Self {
        let (outcome, error) = match applied {
            Ok(Applied::Applied) => (ImportOutcome::Applied, None),
            Ok(Applied::Skipped) => (ImportOutcome::Skipped, None),
            Err(why) => (ImportOutcome::Failed, Some(why)),
        };
        Self {
            media_id: Some(media_id),
            confidence: Some(confidence),
            error,
            ..Self::new(outcome)
        }
    }
}
