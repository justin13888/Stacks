use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, Order,
    QueryFilter, QueryOrder, QuerySelect,
};

use crate::entity::job;

use super::JobError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobType {
    BeamSync,
    Import,
    Notification,
}

impl JobType {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobType::BeamSync => "beam_sync",
            JobType::Import => "import",
            JobType::Notification => "notification",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "beam_sync" => Some(JobType::BeamSync),
            "import" => Some(JobType::Import),
            "notification" => Some(JobType::Notification),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Pending => "pending",
            JobStatus::Running => "running",
            JobStatus::Completed => "completed",
            JobStatus::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(JobStatus::Pending),
            "running" => Some(JobStatus::Running),
            "completed" => Some(JobStatus::Completed),
            "failed" => Some(JobStatus::Failed),
            _ => None,
        }
    }
}

/// Build a fresh pending job, runnable immediately.
pub fn build_job(
    job_type: JobType,
    payload: Option<serde_json::Value>,
    max_attempts: i32,
) -> job::ActiveModel {
    let now = Utc::now();
    job::ActiveModel {
        id: Set(ulid::Ulid::new().to_string()),
        job_type: Set(job_type.as_str().to_string()),
        payload: Set(payload),
        status: Set(JobStatus::Pending.as_str().to_string()),
        attempts: Set(0),
        max_attempts: Set(max_attempts),
        run_at: Set(now.into()),
        started_at: Set(None),
        completed_at: Set(None),
        error: Set(None),
        created_at: Set(now.into()),
    }
}

pub async fn enqueue_job(
    db: &DatabaseConnection,
    model: job::ActiveModel,
) -> Result<job::Model, JobError> {
    model.insert(db).await.map_err(JobError::Db)
}

pub async fn find_job_by_id(
    db: &DatabaseConnection,
    id: &str,
) -> Result<Option<job::Model>, JobError> {
    job::Entity::find_by_id(id)
        .one(db)
        .await
        .map_err(JobError::Db)
}

/// Pending jobs whose `run_at` is due, oldest first — the runner's work queue.
pub async fn find_pending_jobs(
    db: &DatabaseConnection,
    limit: u64,
) -> Result<Vec<job::Model>, JobError> {
    job::Entity::find()
        .filter(job::Column::Status.eq(JobStatus::Pending.as_str()))
        .filter(job::Column::RunAt.lte(Utc::now()))
        .order_by(job::Column::RunAt, Order::Asc)
        .limit(limit)
        .all(db)
        .await
        .map_err(JobError::Db)
}

/// The most recently created jobs — for the admin job list.
pub async fn find_recent_jobs(
    db: &DatabaseConnection,
    limit: u64,
) -> Result<Vec<job::Model>, JobError> {
    job::Entity::find()
        .order_by(job::Column::CreatedAt, Order::Desc)
        .limit(limit)
        .all(db)
        .await
        .map_err(JobError::Db)
}

/// Claim a job: mark it running, stamp `started_at`, and bump `attempts`.
pub async fn mark_running(db: &DatabaseConnection, id: &str) -> Result<job::Model, JobError> {
    let model = find_job_by_id(db, id).await?.ok_or(JobError::NotFound)?;
    let next_attempts = model.attempts + 1;
    let mut active: job::ActiveModel = model.into();
    active.status = Set(JobStatus::Running.as_str().to_string());
    active.attempts = Set(next_attempts);
    active.started_at = Set(Some(Utc::now().into()));
    active.error = Set(None);
    active.update(db).await.map_err(JobError::Db)
}

pub async fn mark_completed(db: &DatabaseConnection, id: &str) -> Result<job::Model, JobError> {
    let model = find_job_by_id(db, id).await?.ok_or(JobError::NotFound)?;
    let mut active: job::ActiveModel = model.into();
    active.status = Set(JobStatus::Completed.as_str().to_string());
    active.completed_at = Set(Some(Utc::now().into()));
    active.error = Set(None);
    active.update(db).await.map_err(JobError::Db)
}

pub async fn mark_failed(
    db: &DatabaseConnection,
    id: &str,
    error: String,
) -> Result<job::Model, JobError> {
    let model = find_job_by_id(db, id).await?.ok_or(JobError::NotFound)?;
    let mut active: job::ActiveModel = model.into();
    active.status = Set(JobStatus::Failed.as_str().to_string());
    active.completed_at = Set(Some(Utc::now().into()));
    active.error = Set(Some(error));
    active.update(db).await.map_err(JobError::Db)
}

/// Return a job to the pending queue after `delay_secs` — used for retry
/// backoff when a run fails but attempts remain.
pub async fn reschedule(
    db: &DatabaseConnection,
    id: &str,
    delay_secs: i64,
    error: Option<String>,
) -> Result<job::Model, JobError> {
    let model = find_job_by_id(db, id).await?.ok_or(JobError::NotFound)?;
    let mut active: job::ActiveModel = model.into();
    active.status = Set(JobStatus::Pending.as_str().to_string());
    active.run_at = Set((Utc::now() + Duration::seconds(delay_secs)).into());
    active.started_at = Set(None);
    active.error = Set(error);
    active.update(db).await.map_err(JobError::Db)
}

/// Manually requeue a job from the admin surface: reset it to pending with a
/// fresh attempt budget.
pub async fn retry_job(db: &DatabaseConnection, id: &str) -> Result<job::Model, JobError> {
    let model = find_job_by_id(db, id).await?.ok_or(JobError::NotFound)?;
    let mut active: job::ActiveModel = model.into();
    active.status = Set(JobStatus::Pending.as_str().to_string());
    active.attempts = Set(0);
    active.run_at = Set(Utc::now().into());
    active.started_at = Set(None);
    active.completed_at = Set(None);
    active.error = Set(None);
    active.update(db).await.map_err(JobError::Db)
}

/// Delete completed jobs older than `older_than_days`. Returns the row count.
pub async fn delete_completed_jobs(
    db: &DatabaseConnection,
    older_than_days: u32,
) -> Result<u64, JobError> {
    let cutoff = Utc::now() - Duration::days(i64::from(older_than_days));
    let result = job::Entity::delete_many()
        .filter(job::Column::Status.eq(JobStatus::Completed.as_str()))
        .filter(job::Column::CompletedAt.lt(cutoff))
        .exec(db)
        .await
        .map_err(JobError::Db)?;
    Ok(result.rows_affected)
}
