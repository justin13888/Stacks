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
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DatabaseTransaction, EntityTrait, Iterable,
    Order, QueryFilter, QueryOrder, QuerySelect, Select, TransactionTrait,
};
use uuid::Uuid;

use crate::db::now;
use crate::entity::enums::{ConflictStrategy, ImportOutcome, ImportSource, ImportStatus, JobType};
use crate::entity::{import, import_item, media};
use crate::import_export::error::ImportExportError;
use crate::import_export::import::apply::{self, Applied, Unapplied};
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
    /// The row cannot take this resolution in its current outcome: it has
    /// not been read yet, or (to skip it) it already has a title.
    #[error("the row is {outcome}; {allowed}")]
    NotResolvable {
        outcome: ImportOutcome,
        /// Which rows the resolution takes.
        allowed: &'static str,
    },
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

/// A row matched by hand ([`ImportPipeline::resolve_item`]).
#[derive(Debug, Clone)]
pub struct Resolved {
    /// The row as it now stands.
    pub item: import_item::Model,
    /// The title it is matched to.
    pub media: media::Model,
    /// What became of the old title's entry, when the row was re-pointed;
    /// `None` when it had no title before, or already had this one.
    pub previous: Option<Unapplied>,
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
    /// When the metadata provider cannot answer a row (it is down or
    /// rate-limiting), that row and every row after it wait as
    /// `pending_retry` without another call, the import goes back to
    /// `pending`, and the report says how many are waiting and how many
    /// this run settled: the job reschedules itself.
    ///
    /// A stored file that no longer parses fails the import for good: the
    /// report's status is then [`ImportStatus::Failed`].
    pub async fn run(&self, import_id: Uuid) -> Result<RunReport, ImportExportError> {
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
                return Ok(RunReport {
                    counts: self.tally(imp.id).await?,
                    settled: 0,
                    status: ImportStatus::Failed,
                });
            }
        };
        self.record_items(imp.id, &entries).await?;

        let matcher = Matcher::new(self.db.clone(), self.metadata.clone(), imp.user_id);
        let mut settled = 0;
        // Why the provider could not answer, once it has said so: the rest
        // of the rows wait for the next run rather than ask it again.
        let mut halted: Option<String> = None;
        for item in items(&self.db, imp.id).await? {
            if !WAITING.contains(&item.outcome) {
                continue;
            }
            // A row resolved by hand meanwhile keeps the outcome it has now,
            // here and below: `settle` only writes a row still waiting.
            if let Some(why) = &halted {
                // A row already waiting keeps its reason; only a row not yet
                // read is written.
                if item.outcome == ImportOutcome::PendingRetry {
                    continue;
                }
                let _ = self
                    .settle(&item, WAITING, Outcome::retry(why.clone()))
                    .await?;
                continue;
            }
            let entry = match row_of(&item) {
                Ok(e) => e,
                Err(e) => {
                    if self
                        .settle(&item, WAITING, Outcome::failed(e))
                        .await?
                        .is_ok()
                    {
                        settled += 1;
                    }
                    continue;
                }
            };
            let outcome = match matcher.resolve(&entry).await {
                Resolution::Matched {
                    media,
                    confidence,
                    candidates,
                } => {
                    let candidates = (!candidates.is_empty())
                        .then(|| serde_json::to_value(&candidates))
                        .transpose()
                        .map_err(|e| ImportExportError::Serialization(e.to_string()))?;
                    let done = self
                        .apply_item(
                            &imp,
                            &item,
                            WAITING,
                            Match {
                                media: &media,
                                confidence,
                                candidates,
                            },
                            &entry,
                            true,
                        )
                        .await?;
                    if done.is_ok() {
                        settled += 1;
                    }
                    continue;
                }
                Resolution::Unmatched(candidates) => Outcome {
                    candidates: Some(
                        serde_json::to_value(candidates)
                            .map_err(|e| ImportExportError::Serialization(e.to_string()))?,
                    ),
                    ..Outcome::new(ImportOutcome::Unmatched)
                },
                Resolution::Retry(why) => {
                    halted = Some(why.clone());
                    let _ = self.settle(&item, WAITING, Outcome::retry(why)).await?;
                    continue;
                }
                Resolution::Failed(why) => Outcome::failed(why),
            };
            if self.settle(&item, WAITING, outcome).await?.is_ok() {
                settled += 1;
            }
        }

        let (counts, status) = self.finish(imp.id).await?;
        Ok(RunReport {
            counts,
            settled,
            status,
        })
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
    /// the import's conflict strategy. The match stays with this user's
    /// import rows, so their next import of the same MAL id matches on its
    /// own; it is never written to the shared `media_refs`, where it would
    /// match every other account's rows too.
    ///
    /// A row the import already matched (or one resolved by hand before) is
    /// re-pointed: what its application did to the old title's entry is
    /// taken back first ([`apply::unapply`]), in the same transaction, and
    /// [`Resolved::previous`] says what became of that entry. A row
    /// already matched to `title` is left as it is. Only a row the import
    /// has not read yet is refused.
    ///
    /// The row is locked while it is applied, so the import job working on
    /// the same row cannot apply it a second time: whichever comes second
    /// finds the row settled.
    pub async fn resolve_item(
        &self,
        import_id: Uuid,
        row_number: i32,
        title: &MediaId,
    ) -> Result<Resolved, ResolveError> {
        let imp = import::Entity::find_by_id(import_id)
            .one(&self.db)
            .await?
            .ok_or(ResolveError::NoSuchRow)?;
        let item = find_item(&self.db, import_id, row_number)
            .await?
            .ok_or(ResolveError::NoSuchRow)?;
        let refused = |outcome| ResolveError::NotResolvable {
            outcome,
            allowed: "only a row the import has read can be matched",
        };
        if !MATCHABLE.contains(&item.outcome) {
            return Err(refused(item.outcome));
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
        let chosen = Match {
            media: &media,
            confidence: 1.0,
            // The row keeps the candidates it was shown.
            candidates: None,
        };
        let (item, previous) = self
            .apply_item(&imp, &item, MATCHABLE, chosen, &entry, false)
            .await?
            .map_err(refused)?;
        self.tally(import_id).await?;
        Ok(Resolved {
            item,
            media,
            previous,
        })
    }

    /// Set a row no title was applied for aside by hand: it becomes
    /// `skipped`, with no title, and keeps its candidates so it can still be
    /// matched later. A row already set aside is left as it is; a row with a
    /// title, or one the import has not read yet, is refused.
    pub async fn skip_item(
        &self,
        import_id: Uuid,
        row_number: i32,
    ) -> Result<import_item::Model, ResolveError> {
        let item = find_item(&self.db, import_id, row_number)
            .await?
            .ok_or(ResolveError::NoSuchRow)?;
        if item.outcome == ImportOutcome::Skipped && item.matched_media_id.is_none() {
            return Ok(item);
        }
        let skipped = self
            .settle(&item, RESOLVABLE, Outcome::new(ImportOutcome::Skipped))
            .await?
            .map_err(|outcome| ResolveError::NotResolvable {
                outcome,
                allowed: "only an unmatched, failed or retrying row can be skipped",
            })?;
        self.tally(import_id).await?;
        Ok(skipped)
    }

    /// Apply a row matched as `chosen` and record its outcome in one
    /// transaction, with the row locked, if its outcome is still one of
    /// `expected`; otherwise `Err` with the outcome it has now, and nothing
    /// is applied.
    ///
    /// A row that already has a title is re-pointed: its earlier
    /// application is taken back first ([`apply::unapply`]), and a row
    /// already matched to `chosen`'s title is returned as it is.
    ///
    /// A row that fails to apply is recorded `failed` with the reason when
    /// `record_failure`; otherwise the failure is returned and the row is
    /// left as it was.
    async fn apply_item(
        &self,
        imp: &import::Model,
        item: &import_item::Model,
        expected: &[ImportOutcome],
        chosen: Match<'_>,
        entry: &ImportEntry,
        record_failure: bool,
    ) -> Result<Result<(import_item::Model, Option<Unapplied>), ImportOutcome>, ImportExportError>
    {
        let Match {
            media,
            confidence,
            candidates,
        } = chosen;
        let txn = self.db.begin().await?;
        let locked = match lock_if(&txn, item.id, expected).await? {
            Ok(locked) => locked,
            Err(now) => {
                txn.rollback().await?;
                return Ok(Err(now));
            }
        };
        let mut unapplied = None;
        if let Some(previous) = locked.matched_media_id
            && matches!(
                locked.outcome,
                ImportOutcome::Applied | ImportOutcome::Skipped
            )
        {
            if previous == media.id {
                txn.rollback().await?;
                return Ok(Ok((locked, None)));
            }
            unapplied =
                Some(apply::unapply(&txn, imp.user_id, previous, imp.id, locked.updated_at).await?);
        }
        let applied = apply::apply(
            &txn,
            imp.user_id,
            media,
            entry,
            imp.conflict_strategy,
            imp.id,
        )
        .await;
        if let Err(why) = &applied
            && !record_failure
        {
            txn.rollback().await?;
            return Err(ImportExportError::Internal(why.clone()));
        }
        let row = write_outcome(
            &txn,
            &locked,
            Outcome {
                candidates,
                ..Outcome::applied(applied, media.id, confidence)
            },
        )
        .await?;
        txn.commit().await?;
        Ok(Ok((row, unapplied)))
    }

    /// Record an outcome that applies nothing, if the row's outcome is still
    /// one of `expected`; otherwise `Err` with the outcome it has now.
    async fn settle(
        &self,
        item: &import_item::Model,
        expected: &[ImportOutcome],
        o: Outcome,
    ) -> Result<Result<import_item::Model, ImportOutcome>, ImportExportError> {
        let txn = self.db.begin().await?;
        let locked = match lock_if(&txn, item.id, expected).await? {
            Ok(locked) => locked,
            Err(now) => {
                txn.rollback().await?;
                return Ok(Err(now));
            }
        };
        let row = write_outcome(&txn, &locked, o).await?;
        txn.commit().await?;
        Ok(Ok(row))
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
    async fn finish(
        &self,
        import_id: Uuid,
    ) -> Result<(ImportReport, ImportStatus), ImportExportError> {
        let report = self.tally(import_id).await?;
        let status = if report.pending_retry > 0 {
            ImportStatus::Pending
        } else {
            ImportStatus::Completed
        };
        let error = (status == ImportStatus::Pending).then(|| {
            format!(
                "{} rows are waiting for the metadata provider; the import retries them",
                report.pending_retry
            )
        });
        self.set_status(import_id, status, error).await?;
        Ok((report, status))
    }
}

/// What one [`ImportPipeline::run`] came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReport {
    /// The import's counts after the run.
    pub counts: ImportReport,
    /// Rows this run gave a final outcome (applied, skipped, unmatched or
    /// failed). A deferred run that settled some has made progress.
    pub settled: u32,
    /// Where the run left the import: completed, pending (rows wait for the
    /// metadata provider), or failed (the stored file no longer parses).
    pub status: ImportStatus,
}

/// Rows the import job works on.
const WAITING: &[ImportOutcome] = &[ImportOutcome::Pending, ImportOutcome::PendingRetry];

/// Rows no title was applied for, which [`ImportPipeline::skip_item`]
/// accepts.
const RESOLVABLE: &[ImportOutcome] = &[
    ImportOutcome::Unmatched,
    ImportOutcome::Failed,
    ImportOutcome::PendingRetry,
];

/// Rows [`ImportPipeline::resolve_item`] accepts: every row the import has
/// read.
const MATCHABLE: &[ImportOutcome] = &[
    ImportOutcome::Unmatched,
    ImportOutcome::Failed,
    ImportOutcome::PendingRetry,
    ImportOutcome::Applied,
    ImportOutcome::Skipped,
];

/// The title a row is applied for, and how it was chosen.
struct Match<'a> {
    media: &'a media::Model,
    confidence: f32,
    /// The candidates to store with the row; `None` keeps the ones it has.
    candidates: Option<serde_json::Value>,
}

/// Lock an import row for the rest of `txn`, if its outcome is one of
/// `expected`; otherwise `Err` with the outcome it has now.
async fn lock_if(
    txn: &DatabaseTransaction,
    item_id: Uuid,
    expected: &[ImportOutcome],
) -> Result<Result<import_item::Model, ImportOutcome>, ImportExportError> {
    let locked = import_item::Entity::find_by_id(item_id)
        .lock_exclusive()
        .one(txn)
        .await?
        .ok_or_else(|| ImportExportError::Internal(format!("import row {item_id} vanished")))?;
    Ok(if expected.contains(&locked.outcome) {
        Ok(locked)
    } else {
        Err(locked.outcome)
    })
}

/// Store a row's outcome.
async fn write_outcome(
    txn: &DatabaseTransaction,
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
    // A matched row keeps the candidates it was shown, if it was resolved
    // by hand from them, unless its match brings its own.
    if o.candidates.is_some()
        || !matches!(o.outcome, ImportOutcome::Applied | ImportOutcome::Skipped)
    {
        am.candidates = Set(o.candidates);
    }
    Ok(am.update(txn).await?)
}

/// Mark an import failed because its job gave up on it (its attempts are
/// spent), unless it already finished. Its waiting rows keep their outcome,
/// so retrying the job resumes it.
pub async fn give_up(
    db: &DatabaseConnection,
    import_id: Uuid,
    why: &str,
) -> Result<(), ImportExportError> {
    import::Entity::update_many()
        .col_expr(
            import::Column::Status,
            Expr::value(ImportStatus::Failed.as_str()),
        )
        .col_expr(
            import::Column::Error,
            Expr::value(format!(
                "the import job gave up ({why}); retry the job to resume the waiting rows"
            )),
        )
        .col_expr(import::Column::CompletedAt, Expr::current_timestamp())
        .filter(import::Column::Id.eq(import_id))
        .filter(import::Column::Status.ne(ImportStatus::Completed))
        .exec(db)
        .await?;
    Ok(())
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

    fn retry(why: String) -> Self {
        Self {
            error: Some(why),
            ..Self::new(ImportOutcome::PendingRetry)
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
