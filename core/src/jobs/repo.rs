use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect,
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

pub async fn find_pending_jobs(
    db: &DatabaseConnection,
    limit: u64,
) -> Result<Vec<job::Model>, JobError> {
    let now = Utc::now();
    job::Entity::find()
        .filter(job::Column::Status.eq(JobStatus::Pending.as_str()))
        .filter(job::Column::RunAt.lte(chrono::DateTime::<chrono::FixedOffset>::from(now)))
        .order_by_asc(job::Column::RunAt)
        .limit(limit)
        .all(db)
        .await
        .map_err(JobError::Db)
}

pub async fn list_recent_jobs(
    db: &DatabaseConnection,
    limit: u64,
) -> Result<Vec<job::Model>, JobError> {
    job::Entity::find()
        .order_by_desc(job::Column::CreatedAt)
        .limit(limit)
        .all(db)
        .await
        .map_err(JobError::Db)
}

pub async fn update_job_status(
    db: &DatabaseConnection,
    id: &str,
    status: JobStatus,
    error: Option<String>,
) -> Result<job::Model, JobError> {
    let existing = find_job_by_id(db, id).await?.ok_or(JobError::NotFound)?;
    let now = Utc::now();
    let mut active: job::ActiveModel = existing.into();
    let status_str = status.as_str().to_string();
    match &status {
        JobStatus::Running => {
            active.started_at = Set(Some(now.into()));
            active.attempts = Set(match active.attempts {
                sea_orm::ActiveValue::Set(v) => v + 1,
                sea_orm::ActiveValue::Unchanged(v) => v + 1,
                _ => 1,
            });
        }
        JobStatus::Completed => {
            active.completed_at = Set(Some(now.into()));
        }
        JobStatus::Failed => {
            active.completed_at = Set(Some(now.into()));
        }
        JobStatus::Pending => {
            // Re-enqueue: clear started_at so the next worker can claim it.
            active.started_at = Set(None);
        }
    }
    active.status = Set(status_str);
    active.error = Set(error);
    active.update(db).await.map_err(JobError::Db)
}

pub async fn delete_completed_jobs(
    db: &DatabaseConnection,
    older_than_days: u32,
) -> Result<u64, JobError> {
    let cutoff = Utc::now() - chrono::Duration::days(older_than_days as i64);
    let res = job::Entity::delete_many()
        .filter(job::Column::Status.eq(JobStatus::Completed.as_str()))
        .filter(job::Column::CompletedAt.lt(chrono::DateTime::<chrono::FixedOffset>::from(cutoff)))
        .exec(db)
        .await
        .map_err(JobError::Db)?;
    Ok(res.rows_affected)
}
