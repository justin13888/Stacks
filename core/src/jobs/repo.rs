use sea_orm::DatabaseConnection;

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
    let _ = (db, model);
    todo!()
}

pub async fn find_job_by_id(
    db: &DatabaseConnection,
    id: &str,
) -> Result<Option<job::Model>, JobError> {
    let _ = (db, id);
    todo!()
}

pub async fn find_pending_jobs(
    db: &DatabaseConnection,
    limit: u64,
) -> Result<Vec<job::Model>, JobError> {
    let _ = (db, limit);
    todo!()
}

pub async fn update_job_status(
    db: &DatabaseConnection,
    id: &str,
    status: JobStatus,
    error: Option<String>,
) -> Result<job::Model, JobError> {
    let _ = (db, id, status, error);
    todo!()
}

pub async fn delete_completed_jobs(
    db: &DatabaseConnection,
    older_than_days: u32,
) -> Result<u64, JobError> {
    let _ = (db, older_than_days);
    todo!()
}
