//! Administering the deployment: accounts and the background job queue.
//!
//! Every operation here takes the [`Administrator`] guard: an administrator's
//! session, or an administrator's API token holding `admin`.

use std::sync::Arc;

use kynos::prelude::*;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use stacks_core::auth::AuthService;
use stacks_core::entity::enums::{JobStatus as CoreJobStatus, JobType as CoreJobType};
use stacks_core::entity::{job, user};
use stacks_core::jobs::{JobError, repo as jobs_repo};
use stacks_core::user::{UserError, UserService};
use uuid::Uuid;

use crate::account::Role;
use crate::app::App;
use crate::page::{self, InvalidCursor, Page, PageParams};
use crate::problem::Internal;
use crate::security::{Administrator, Caller};

/// Administering the deployment.
#[derive(Tag)]
#[tag(
    name = "admin",
    description = "Accounts and the background job queue, for administrators."
)]
pub struct Admin;

// ------------------------------------------------------------------ accounts

/// An account, as an administrator sees it.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct AdminUser {
    pub id: Uuid,
    pub display_name: String,
    /// The email the identity provider reported, if any.
    pub email: Option<String>,
    pub role: Role,
    /// When the account was deactivated; `null` while it is active.
    pub deactivated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<&user::Model> for AdminUser {
    fn from(u: &user::Model) -> Self {
        Self {
            id: u.id,
            display_name: u.display_name.clone(),
            email: u.email.clone(),
            role: u.role.into(),
            deactivated_at: u.deactivated_at.map(|d| d.to_utc()),
            created_at: u.created_at.to_utc(),
        }
    }
}

/// A change to an account. Members left out are unchanged.
#[derive(Debug, Default, Schema, Serialize, Deserialize)]
pub struct AdminUserPatch {
    pub role: Option<Role>,
    /// `true` stops the account authenticating and ends every one of its
    /// sessions; `false` lets it sign in again. Its data is untouched either
    /// way.
    pub deactivated: Option<bool>,
}

/// What `/admin/users/{id}` captures.
#[derive(Debug, Schema, PathParams)]
pub struct UserPath {
    pub id: Uuid,
}

/// What a listing can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum ListError {
    #[error(transparent)]
    #[problem(transparent)]
    InvalidCursor(#[from] InvalidCursor),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What changing an account can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
#[problem(base = crate::problem::BASE)]
pub enum UpdateUserError {
    /// No account has that id.
    #[error("no account has that id")]
    #[problem(status = 404, title = "User not found")]
    UserNotFound,
    /// An administrator tried to demote or deactivate their own account.
    #[error("an administrator cannot demote or deactivate their own account")]
    #[problem(status = 409, title = "Cannot change own account")]
    CannotChangeOwnAccount,
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// The SQL page after the request's cursor: one row more than the limit, so
/// [`page::take`] can tell whether another page follows.
fn sql_page(params: &PageParams) -> Result<(Option<Uuid>, u64), InvalidCursor> {
    Ok((params.after::<Uuid>()?, u64::from(params.limit()) + 1))
}

/// List every account, newest first.
#[kynos::get("/admin/users", operation_id = "listUsers")]
async fn list_users(
    Auth(_caller): Administrator,
    Query(params): Query<PageParams>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<Json<Page<AdminUser>>, ListError> {
    let (before, limit) = sql_page(&params)?;
    let users = UserService::new(db)
        .page(before, limit)
        .await
        .map_err(Internal::log)?;
    let page = page::take(users, params.limit(), |u| page::encode(&u.id));
    Ok(Json(page.map(|u| AdminUser::from(&u))))
}

/// Change an account's role, or deactivate or reactivate it.
///
/// Deactivating ends every session of the account at once, and its API
/// tokens are refused while it stays deactivated. An administrator cannot
/// demote or deactivate their own account.
#[kynos::patch("/admin/users/{id}", operation_id = "updateUser")]
async fn update_user(
    Auth(caller): Administrator,
    Path(path): Path<UserPath>,
    Inject(db): Inject<DatabaseConnection>,
    Inject(auth): Inject<Arc<AuthService>>,
    Json(patch): Json<AdminUserPatch>,
) -> Result<Json<AdminUser>, UpdateUserError> {
    let caller = Caller::from(caller);
    let updated = UserService::new(db)
        .administer(
            caller.id(),
            path.id,
            patch.role.map(Into::into),
            patch.deactivated,
        )
        .await
        .map_err(|e| match e {
            UserError::NotFound => UpdateUserError::UserNotFound,
            UserError::CannotChangeOwnAccount => UpdateUserError::CannotChangeOwnAccount,
            other => Internal::log(other).into(),
        })?;
    if patch.deactivated == Some(true) {
        auth.end_all_sessions(updated.id)
            .await
            .map_err(Internal::log)?;
    }
    Ok(Json(AdminUser::from(&updated)))
}

// ---------------------------------------------------------------------- jobs

/// What a background job does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    /// Processes an uploaded import.
    Import,
    /// Delivers a notification.
    Notification,
    /// Refreshes stale catalog snapshots from the providers.
    MetadataRefresh,
}

impl From<CoreJobType> for JobType {
    fn from(t: CoreJobType) -> Self {
        match t {
            CoreJobType::Import => Self::Import,
            CoreJobType::Notification => Self::Notification,
            CoreJobType::MetadataRefresh => Self::MetadataRefresh,
        }
    }
}

/// Where a job is in the queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, Serialize, Deserialize, kynos::ParamValue)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    /// Waiting for `run_at`, or for a worker.
    Pending,
    /// Claimed by a worker.
    Running,
    Completed,
    /// Out of attempts; `error` says why. Only a failed job can be retried.
    Failed,
}

impl From<CoreJobStatus> for JobStatus {
    fn from(s: CoreJobStatus) -> Self {
        match s {
            CoreJobStatus::Pending => Self::Pending,
            CoreJobStatus::Running => Self::Running,
            CoreJobStatus::Completed => Self::Completed,
            CoreJobStatus::Failed => Self::Failed,
        }
    }
}

impl From<JobStatus> for CoreJobStatus {
    fn from(s: JobStatus) -> Self {
        match s {
            JobStatus::Pending => Self::Pending,
            JobStatus::Running => Self::Running,
            JobStatus::Completed => Self::Completed,
            JobStatus::Failed => Self::Failed,
        }
    }
}

/// A background job. Its payload is not shown: it can carry what a job
/// needs and nobody else should read.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct Job {
    pub id: Uuid,
    #[serde(rename = "type")]
    pub job_type: JobType,
    pub status: JobStatus,
    /// Attempts made so far.
    pub attempts: i32,
    /// Attempts allowed before the job fails for good.
    pub max_attempts: i32,
    /// When the job is next due to run.
    pub run_at: chrono::DateTime<chrono::Utc>,
    /// When the current or last attempt started.
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    /// When the job completed or failed for good.
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// The last attempt's failure, if any.
    pub error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<&job::Model> for Job {
    fn from(j: &job::Model) -> Self {
        Self {
            id: j.id,
            job_type: j.job_type.into(),
            status: j.status.into(),
            attempts: j.attempts,
            max_attempts: j.max_attempts,
            run_at: j.run_at.to_utc(),
            started_at: j.started_at.map(|d| d.to_utc()),
            completed_at: j.completed_at.map(|d| d.to_utc()),
            error: j.error.clone(),
            created_at: j.created_at.to_utc(),
        }
    }
}

/// How the job list is filtered and paged.
#[derive(Debug, Default, Schema, QueryParams)]
pub struct JobListParams {
    /// Only jobs in this state.
    pub status: Option<JobStatus>,
    /// The `next_cursor` of the previous page. Omit for the first page.
    pub cursor: Option<String>,
    /// At most this many items (default 50).
    #[schema(minimum = 1, maximum = 200)]
    pub limit: Option<u32>,
}

/// What `/admin/jobs/{id}…` captures.
#[derive(Debug, Schema, PathParams)]
pub struct JobPath {
    pub id: Uuid,
}

/// What retrying a job can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
#[problem(base = crate::problem::BASE)]
pub enum RetryJobError {
    /// No job has that id.
    #[error("no job has that id")]
    #[problem(status = 404, title = "Job not found")]
    JobNotFound,
    /// The job is not failed: it is still queued, running, or done.
    #[error("only a failed job can be retried; this one is {job_status}")]
    #[problem(status = 409, title = "Job not retryable")]
    JobNotRetryable {
        /// The job's current state.
        #[problem(extension)]
        job_status: String,
    },
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// List background jobs, newest first.
#[kynos::get("/admin/jobs", operation_id = "listJobs")]
async fn list_jobs(
    Auth(_caller): Administrator,
    Query(params): Query<JobListParams>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<Json<Page<Job>>, ListError> {
    let paging = PageParams {
        cursor: params.cursor,
        limit: params.limit,
    };
    let (before, limit) = sql_page(&paging)?;
    let jobs = jobs_repo::page_jobs(&db, params.status.map(Into::into), before, limit)
        .await
        .map_err(Internal::log)?;
    let page = page::take(jobs, paging.limit(), |j| page::encode(&j.id));
    Ok(Json(page.map(|j| Job::from(&j))))
}

/// Retry a failed job: it is queued again, due now, with a fresh attempt
/// budget.
#[kynos::post("/admin/jobs/{id}/retries", operation_id = "retryJob")]
async fn retry_job(
    Auth(_caller): Administrator,
    Path(path): Path<JobPath>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<Accepted<Json<Job>>, RetryJobError> {
    let job = jobs_repo::retry_job(&db, path.id)
        .await
        .map_err(|e| match e {
            JobError::NotFound => RetryJobError::JobNotFound,
            JobError::NotRetryable(status) => RetryJobError::JobNotRetryable {
                job_status: status.as_str().to_owned(),
            },
            other => Internal::log(other).into(),
        })?;
    Ok(Accepted::new(Json(Job::from(&job))))
}

pub(crate) fn group() -> Group<App> {
    Group::new("/").tag::<Admin>().mount(kynos::routes![
        list_users,
        update_user,
        list_jobs,
        retry_job,
    ])
}
