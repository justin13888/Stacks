use salvo::prelude::*;
use stacks_core::entity::job;
use stacks_core::jobs::JobError;
use stacks_core::jobs::repo as jobs_repo;

use crate::app::AppState;
use crate::extractors::uuid_param;

use super::dto::*;
use super::error::*;

fn job_to_dto(j: job::Model) -> AdminJobResponse {
    AdminJobResponse {
        id: j.id.to_string(),
        job_type: j.job_type.to_string(),
        status: j.status.to_string(),
        attempts: j.attempts.max(0) as u32,
        created_at: j.created_at.to_rfc3339(),
        error: j.error,
    }
}

/// List the most recent background jobs (admin).
#[salvo::oapi::endpoint(tags("Admin"), summary = "List jobs (admin)")]
pub async fn list_jobs(depot: &mut Depot) -> Result<Json<AdminJobListResponse>, AdminJobError> {
    let state = depot.obtain::<AppState>().unwrap().clone();
    let jobs = jobs_repo::find_recent_jobs(&state.db, 100)
        .await
        .map_err(|e| AdminJobError::Internal(crate::internal!(e)))?;
    Ok(Json(AdminJobListResponse {
        jobs: jobs.into_iter().map(job_to_dto).collect(),
    }))
}

/// Requeue a failed background job (admin).
#[salvo::oapi::endpoint(tags("Admin"), summary = "Retry job (admin)")]
pub async fn retry_job(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<AdminJobResponse>, AdminJobError> {
    let state = depot.obtain::<AppState>().unwrap().clone();
    let id = uuid_param(req, "id").ok_or(AdminJobError::NotFound)?;
    let job = jobs_repo::retry_job(&state.db, id)
        .await
        .map_err(|e| match e {
            JobError::NotFound => AdminJobError::NotFound,
            e => AdminJobError::Internal(crate::internal!(e)),
        })?;
    Ok(Json(job_to_dto(job)))
}
