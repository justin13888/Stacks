//! The Postgres job queue.
//!
//! Workers claim due jobs with `FOR UPDATE SKIP LOCKED`, so any number of
//! runners (or API replicas) can poll the same table without ever running a
//! job twice: a row one claimer has locked is invisible to the others, and the
//! claim flips it to `running` before the lock is released.

use chrono::{DateTime, Duration, Utc};
use sea_orm::ActiveValue::{NotSet, Set, Unchanged};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DbBackend, DbErr, EntityTrait,
    Order, QueryFilter, QueryOrder, QuerySelect, Statement,
};
use uuid::Uuid;

use crate::config::models::JobsConfig;
use crate::entity::job;

pub use crate::entity::enums::{JobStatus, JobType};

use super::JobError;

/// Build a fresh pending job, runnable immediately.
pub fn build_job(
    job_type: JobType,
    payload: Option<serde_json::Value>,
    max_attempts: i32,
) -> job::ActiveModel {
    job::ActiveModel {
        id: Set(Uuid::now_v7()),
        job_type: Set(job_type),
        payload: Set(payload),
        status: Set(JobStatus::Pending),
        attempts: Set(0),
        max_attempts: Set(max_attempts.max(1)),
        run_at: Set(Utc::now().into()),
        started_at: Set(None),
        completed_at: Set(None),
        error: Set(None),
        created_at: NotSet,
        updated_at: NotSet,
    }
}

pub async fn enqueue_job<C: ConnectionTrait>(
    db: &C,
    model: job::ActiveModel,
) -> Result<job::Model, JobError> {
    Ok(model.insert(db).await?)
}

/// Enqueue a job of `job_type` unless one is already pending or running.
/// Returns the new job, or `None` when one was already queued. A worker that
/// shuts down cleanly [`release`]s its job; a `running` row left behind by
/// one that crashed blocks this until [`reclaim_stale_jobs`] recovers it.
///
/// Two replicas racing here can both enqueue; for the periodic jobs this is
/// used for (metadata refresh) a duplicate run is harmless.
pub async fn enqueue_unless_queued<C: ConnectionTrait>(
    db: &C,
    job_type: JobType,
    payload: Option<serde_json::Value>,
    max_attempts: i32,
) -> Result<Option<job::Model>, JobError> {
    let queued = job::Entity::find()
        .filter(job::Column::JobType.eq(job_type))
        .filter(job::Column::Status.is_in([JobStatus::Pending, JobStatus::Running]))
        .one(db)
        .await?;
    if queued.is_some() {
        return Ok(None);
    }
    Ok(Some(
        enqueue_job(db, build_job(job_type, payload, max_attempts)).await?,
    ))
}

pub async fn find_job_by_id<C: ConnectionTrait>(
    db: &C,
    id: Uuid,
) -> Result<Option<job::Model>, JobError> {
    Ok(job::Entity::find_by_id(id).one(db).await?)
}

/// Atomically claim up to `limit` due pending jobs, oldest `run_at` first:
/// each comes back `running` with `attempts` incremented.
pub async fn claim_due_jobs<C: ConnectionTrait>(
    db: &C,
    limit: u64,
) -> Result<Vec<job::Model>, JobError> {
    let sql = r#"
        UPDATE jobs SET
            status = 'running',
            attempts = attempts + 1,
            started_at = now(),
            error = NULL
        WHERE id IN (
            SELECT id FROM jobs
            WHERE status = 'pending' AND run_at <= now()
            ORDER BY run_at
            LIMIT $1
            FOR UPDATE SKIP LOCKED
        )
        RETURNING *
    "#;
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    Ok(job::Entity::find()
        .from_raw_sql(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            [limit.into()],
        ))
        .all(db)
        .await?)
}

/// Recover jobs a worker claimed and never finished: a row still `running`
/// whose `started_at` is more than `stale_after_secs` old. The process that
/// claimed it is presumed gone (a restart or crash mid-run), so the job goes
/// back to `pending`, runnable now, when attempts remain, and is marked
/// `failed` when its attempt budget is spent. Returns the rows recovered, as
/// they are now.
///
/// Without this a restart would strand the row as `running` forever — and
/// [`enqueue_unless_queued`] would then never queue that job type again.
pub async fn reclaim_stale_jobs<C: ConnectionTrait>(
    db: &C,
    stale_after_secs: i64,
) -> Result<Vec<job::Model>, JobError> {
    let sql = r#"
        UPDATE jobs SET
            status = CASE WHEN attempts < max_attempts THEN 'pending' ELSE 'failed' END,
            run_at = CASE WHEN attempts < max_attempts THEN now() ELSE run_at END,
            completed_at = CASE WHEN attempts < max_attempts THEN NULL ELSE now() END,
            started_at = NULL,
            error = 'abandoned: the worker stopped before the job finished'
        WHERE status = 'running'
          AND started_at < now() - make_interval(secs => $1)
        RETURNING *
    "#;
    Ok(job::Entity::find()
        .from_raw_sql(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            [(stale_after_secs as f64).into()],
        ))
        .all(db)
        .await?)
}

async fn write<C: ConnectionTrait>(db: &C, am: job::ActiveModel) -> Result<job::Model, JobError> {
    am.update(db).await.map_err(|e| match e {
        DbErr::RecordNotUpdated => JobError::NotFound,
        e => JobError::Db(e),
    })
}

pub async fn mark_completed<C: ConnectionTrait>(db: &C, id: Uuid) -> Result<job::Model, JobError> {
    write(
        db,
        job::ActiveModel {
            id: Unchanged(id),
            status: Set(JobStatus::Completed),
            completed_at: Set(Some(Utc::now().into())),
            error: Set(None),
            ..Default::default()
        },
    )
    .await
}

pub async fn mark_failed<C: ConnectionTrait>(
    db: &C,
    id: Uuid,
    error: String,
) -> Result<job::Model, JobError> {
    write(
        db,
        job::ActiveModel {
            id: Unchanged(id),
            status: Set(JobStatus::Failed),
            completed_at: Set(Some(Utc::now().into())),
            error: Set(Some(error)),
            ..Default::default()
        },
    )
    .await
}

/// Return a job to the pending queue after `delay_secs` — used for retry
/// backoff when a run fails but attempts remain.
pub async fn reschedule<C: ConnectionTrait>(
    db: &C,
    id: Uuid,
    delay_secs: i64,
    error: Option<String>,
) -> Result<job::Model, JobError> {
    write(
        db,
        job::ActiveModel {
            id: Unchanged(id),
            status: Set(JobStatus::Pending),
            run_at: Set((Utc::now() + Duration::seconds(delay_secs)).into()),
            started_at: Set(None),
            error: Set(error),
            ..Default::default()
        },
    )
    .await
}

/// Return a job that made progress to the pending queue after `delay_secs`,
/// giving back the attempt this run was claimed with: a job that keeps
/// advancing is never given up on.
pub async fn defer<C: ConnectionTrait>(
    db: &C,
    id: Uuid,
    delay_secs: i64,
    error: String,
) -> Result<job::Model, JobError> {
    let updated = job::Entity::update_many()
        .col_expr(
            job::Column::Status,
            Expr::value(JobStatus::Pending.as_str()),
        )
        .col_expr(
            job::Column::Attempts,
            Expr::cust("GREATEST(attempts - 1, 0)"),
        )
        .col_expr(
            job::Column::RunAt,
            Expr::value(Utc::now() + Duration::seconds(delay_secs)),
        )
        .col_expr(
            job::Column::StartedAt,
            Expr::value(Option::<chrono::DateTime<Utc>>::None),
        )
        .col_expr(job::Column::Error, Expr::value(Some(error)))
        .filter(job::Column::Id.eq(id))
        .exec_with_returning(db)
        .await?;
    updated.into_iter().next().ok_or(JobError::NotFound)
}

/// Hand a claimed job back unrun: a worker that is shutting down returns it
/// to `pending`, runnable now, and gives back the attempt the claim spent,
/// because the job did not fail. Only a `running` row is released.
pub async fn release<C: ConnectionTrait>(db: &C, id: Uuid) -> Result<job::Model, JobError> {
    let updated = job::Entity::update_many()
        .col_expr(
            job::Column::Status,
            Expr::value(JobStatus::Pending.as_str()),
        )
        .col_expr(
            job::Column::Attempts,
            Expr::cust("GREATEST(attempts - 1, 0)"),
        )
        .col_expr(job::Column::RunAt, Expr::value(Utc::now()))
        .col_expr(
            job::Column::StartedAt,
            Expr::value(Option::<chrono::DateTime<Utc>>::None),
        )
        .filter(job::Column::Id.eq(id))
        .filter(job::Column::Status.eq(JobStatus::Running))
        .exec_with_returning(db)
        .await?;
    updated.into_iter().next().ok_or(JobError::NotFound)
}

/// Manually requeue a failed job from the admin surface: back to pending,
/// runnable now, with a fresh attempt budget.
///
/// Only a `failed` job is retried. A pending or running job is still queued
/// (resetting it would hand a running job to a second worker), and a
/// completed one has nothing left to do. The status check and the reset are
/// one conditional `UPDATE`, so a job a worker claims concurrently is never
/// reset under it.
pub async fn retry_job<C: ConnectionTrait>(db: &C, id: Uuid) -> Result<job::Model, JobError> {
    let updated = job::Entity::update_many()
        .col_expr(
            job::Column::Status,
            Expr::value(JobStatus::Pending.as_str()),
        )
        .col_expr(job::Column::Attempts, Expr::value(0))
        .col_expr(job::Column::RunAt, Expr::current_timestamp())
        .col_expr(
            job::Column::StartedAt,
            Expr::value(Option::<chrono::DateTime<Utc>>::None),
        )
        .col_expr(
            job::Column::CompletedAt,
            Expr::value(Option::<chrono::DateTime<Utc>>::None),
        )
        .col_expr(job::Column::Error, Expr::value(Option::<String>::None))
        .filter(job::Column::Id.eq(id))
        .filter(job::Column::Status.eq(JobStatus::Failed))
        .exec_with_returning(db)
        .await?;
    if let Some(job) = updated.into_iter().next() {
        return Ok(job);
    }
    match find_job_by_id(db, id).await? {
        Some(job) => Err(JobError::NotRetryable(job.status)),
        None => Err(JobError::NotFound),
    }
}

/// Which jobs [`page_jobs`] lists; `None` members do not filter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct JobFilter {
    pub status: Option<JobStatus>,
    pub job_type: Option<JobType>,
}

/// One page of jobs, newest first (ids are UUIDv7, so time-ordered), before
/// `before` when given and only those `filter` admits. Returns at most
/// `limit` rows; ask for one more than a page to learn whether another
/// follows.
pub async fn page_jobs<C: ConnectionTrait>(
    db: &C,
    filter: JobFilter,
    before: Option<Uuid>,
    limit: u64,
) -> Result<Vec<job::Model>, JobError> {
    let mut query = job::Entity::find().order_by(job::Column::Id, Order::Desc);
    if let Some(status) = filter.status {
        query = query.filter(job::Column::Status.eq(status));
    }
    if let Some(job_type) = filter.job_type {
        query = query.filter(job::Column::JobType.eq(job_type));
    }
    if let Some(before) = before {
        query = query.filter(job::Column::Id.lt(before));
    }
    Ok(query.limit(limit).all(db).await?)
}

/// Delete finished jobs past their retention: `completed` jobs that finished
/// more than `retention.completed_retention_days` ago and `failed` jobs more
/// than `retention.failed_retention_days` ago. A retention of `0` keeps that
/// status forever. Returns the row count.
///
/// Jobs are operational records, not user data. Only `completed` and `failed`
/// rows are ever matched, so a pending or running job is never deleted, and a
/// failed job an admin retried is pending again and safe.
pub async fn prune_finished_jobs<C: ConnectionTrait>(
    db: &C,
    retention: JobsConfig,
) -> Result<u64, JobError> {
    let now = Utc::now();
    let mut expired = Condition::any();
    for (status, days) in [
        (JobStatus::Completed, retention.completed_retention_days),
        (JobStatus::Failed, retention.failed_retention_days),
    ] {
        if let Some(cutoff) = retention_cutoff(now, days) {
            expired = expired.add(
                Condition::all()
                    .add(job::Column::Status.eq(status))
                    .add(job::Column::CompletedAt.lt(cutoff)),
            );
        }
    }
    if expired.is_empty() {
        return Ok(0);
    }
    let result = job::Entity::delete_many().filter(expired).exec(db).await?;
    Ok(result.rows_affected)
}

/// The instant `days` before `now`, or `None` when nothing is old enough to
/// prune: `0` (keep forever) or a span past what a timestamp can hold.
fn retention_cutoff(now: DateTime<Utc>, days: u64) -> Option<DateTime<Utc>> {
    if days == 0 {
        return None;
    }
    let span = Duration::try_days(i64::try_from(days).ok()?)?;
    now.checked_sub_signed(span)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zero_or_unrepresentable_retention_prunes_nothing() {
        let now = Utc::now();
        assert_eq!(retention_cutoff(now, 0), None);
        assert_eq!(retention_cutoff(now, u64::MAX), None);
        assert_eq!(retention_cutoff(now, 30), Some(now - Duration::days(30)));
    }
}
