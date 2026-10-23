//! The Postgres job queue.
//!
//! Workers claim due jobs with `FOR UPDATE SKIP LOCKED`, so any number of
//! runners (or API replicas) can poll the same table without ever running a
//! job twice: a row one claimer has locked is invisible to the others, and the
//! claim flips it to `running` before the lock is released.

use chrono::{Duration, Utc};
use sea_orm::ActiveValue::{NotSet, Set, Unchanged};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, DbErr, EntityTrait, Order,
    QueryFilter, QueryOrder, QuerySelect, Statement,
};
use uuid::Uuid;

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
/// Returns the new job, or `None` when one was already queued. A `running`
/// row left behind by a stopped worker only blocks this until
/// [`reclaim_stale_jobs`] recovers it.
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
/// `failed` when its attempt budget is spent. Returns the number of rows
/// recovered.
///
/// Without this a restart would strand the row as `running` forever — and
/// [`enqueue_unless_queued`] would then never queue that job type again.
pub async fn reclaim_stale_jobs<C: ConnectionTrait>(
    db: &C,
    stale_after_secs: i64,
) -> Result<u64, JobError> {
    let sql = r#"
        UPDATE jobs SET
            status = CASE WHEN attempts < max_attempts THEN 'pending' ELSE 'failed' END,
            run_at = CASE WHEN attempts < max_attempts THEN now() ELSE run_at END,
            completed_at = CASE WHEN attempts < max_attempts THEN NULL ELSE now() END,
            started_at = NULL,
            error = 'abandoned: the worker stopped before the job finished'
        WHERE status = 'running'
          AND started_at < now() - make_interval(secs => $1)
    "#;
    let result = db
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            [(stale_after_secs as f64).into()],
        ))
        .await?;
    Ok(result.rows_affected())
}

/// The most recently created jobs — for the admin job list.
pub async fn find_recent_jobs<C: ConnectionTrait>(
    db: &C,
    limit: u64,
) -> Result<Vec<job::Model>, JobError> {
    Ok(job::Entity::find()
        .order_by(job::Column::Id, Order::Desc)
        .limit(limit)
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

/// Manually requeue a job from the admin surface: reset it to pending with a
/// fresh attempt budget.
pub async fn retry_job<C: ConnectionTrait>(db: &C, id: Uuid) -> Result<job::Model, JobError> {
    write(
        db,
        job::ActiveModel {
            id: Unchanged(id),
            status: Set(JobStatus::Pending),
            attempts: Set(0),
            run_at: Set(Utc::now().into()),
            started_at: Set(None),
            completed_at: Set(None),
            error: Set(None),
            ..Default::default()
        },
    )
    .await
}

/// Delete completed jobs older than `older_than_days`. Returns the row count.
/// Jobs are operational records, not user data.
pub async fn delete_completed_jobs<C: ConnectionTrait>(
    db: &C,
    older_than_days: u32,
) -> Result<u64, JobError> {
    let cutoff = Utc::now() - Duration::days(i64::from(older_than_days));
    let result = job::Entity::delete_many()
        .filter(job::Column::Status.eq(JobStatus::Completed))
        .filter(job::Column::CompletedAt.lt(cutoff))
        .exec(db)
        .await?;
    Ok(result.rows_affected)
}
