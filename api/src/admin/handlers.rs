use salvo::prelude::*;
use stacks_core::crypto;
use stacks_core::entity::{job, streaming_server};
use stacks_core::jobs::JobError;
use stacks_core::jobs::repo as jobs_repo;
use stacks_core::streaming::{StreamingError, StreamingService};

use crate::app::AppState;
use crate::error::IntoInternal;

use super::dto::*;
use super::error::*;

/// Build a `StreamingService` with the credential-encryption key derived from
/// the application secret.
fn streaming_service(state: &AppState) -> StreamingService {
    StreamingService::new(
        state.db.clone(),
        crypto::derive_key(&state.config.auth.secret_key),
    )
}

fn server_to_dto(s: streaming_server::Model) -> AdminServerResponse {
    AdminServerResponse {
        id: s.id,
        name: s.name,
        server_type: s.server_type,
        base_url: s.base_url,
        enabled: s.enabled,
        last_synced_at: s.last_synced_at.map(|t| t.to_rfc3339()),
    }
}

fn job_to_dto(j: job::Model) -> AdminJobResponse {
    AdminJobResponse {
        id: j.id,
        job_type: j.job_type,
        status: j.status,
        attempts: j.attempts.max(0) as u32,
        created_at: j.created_at.to_rfc3339(),
        error: j.error,
    }
}

/// List all registered streaming servers (admin).
#[salvo::oapi::endpoint(tags("Admin"), summary = "List servers (admin)")]
pub async fn list_servers(
    depot: &mut Depot,
) -> Result<Json<Vec<AdminServerResponse>>, AdminCreateServerError> {
    let state = depot.obtain::<AppState>().unwrap().clone();
    let servers = streaming_service(&state)
        .list_all_servers()
        .await
        .map_err(|e| AdminCreateServerError::Internal(crate::internal!(e)))?;
    Ok(Json(servers.into_iter().map(server_to_dto).collect()))
}

/// Register a new streaming server (admin).
#[salvo::oapi::endpoint(tags("Admin"), summary = "Create server (admin)")]
pub async fn create_server(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<AdminServerResponse>, AdminCreateServerError> {
    let state = depot.obtain::<AppState>().unwrap().clone();
    let body: AdminCreateServerRequest = req.parse_json().await.into_internal()?;
    let server = streaming_service(&state)
        .register_server(
            None,
            body.name,
            body.server_type,
            body.base_url,
            body.api_key,
            body.webhook_secret,
        )
        .await
        .map_err(|e| match e {
            StreamingError::UnsupportedServerType(t) => {
                AdminCreateServerError::Unsupported(format!("unsupported server type: {t}"))
            }
            e => AdminCreateServerError::Internal(crate::internal!(e)),
        })?;
    Ok(Json(server_to_dto(server)))
}

/// Update a streaming server's configuration (admin).
#[salvo::oapi::endpoint(tags("Admin"), summary = "Update server (admin)")]
pub async fn update_server(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<AdminServerResponse>, AdminDeleteServerError> {
    let state = depot.obtain::<AppState>().unwrap().clone();
    let id = req.param::<String>("id").unwrap_or_default();
    let body: AdminUpdateServerRequest = req.parse_json().await.into_internal()?;
    let server = streaming_service(&state)
        .update_server(
            &id,
            body.name,
            body.base_url,
            body.api_key,
            body.webhook_secret,
            body.enabled,
        )
        .await
        .map_err(|e| match e {
            StreamingError::ServerNotFound => AdminDeleteServerError::NotFound,
            e => AdminDeleteServerError::Internal(crate::internal!(e)),
        })?;
    Ok(Json(server_to_dto(server)))
}

/// Delete a streaming server (admin).
#[salvo::oapi::endpoint(tags("Admin"), summary = "Delete server (admin)")]
pub async fn delete_server(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<()>, AdminDeleteServerError> {
    let state = depot.obtain::<AppState>().unwrap().clone();
    let id = req.param::<String>("id").unwrap_or_default();
    streaming_service(&state)
        .delete_server(&id)
        .await
        .map_err(|e| match e {
            StreamingError::ServerNotFound => AdminDeleteServerError::NotFound,
            e => AdminDeleteServerError::Internal(crate::internal!(e)),
        })?;
    Ok(Json(()))
}

/// Trigger a manual metadata sync for a server (admin).
#[salvo::oapi::endpoint(tags("Admin"), summary = "Sync server (admin)")]
pub async fn sync_server(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<AdminSyncResponse>, AdminSyncError> {
    let state = depot.obtain::<AppState>().unwrap().clone();
    let id = req.param::<String>("id").unwrap_or_default();

    // Confirm the server exists before enqueuing, so an unknown id is a 404
    // rather than a job that fails asynchronously.
    streaming_service(&state)
        .get_server(&id)
        .await
        .map_err(|e| match e {
            StreamingError::ServerNotFound => AdminSyncError::NotFound,
            e => AdminSyncError::Internal(crate::internal!(e)),
        })?;

    // Enqueue rather than syncing inline. A library sync is an unbounded call
    // to someone else's media server: run inline it blocked the request and
    // turned an unreachable server into a 500, with no retry. The JobRunner
    // already owns backoff and the attempt budget, and the periodic scheduler
    // enqueues the very same job type.
    let job = jobs_repo::build_job(
        jobs_repo::JobType::BeamSync,
        Some(serde_json::json!({ "server_id": id })),
        3,
    );
    let job = jobs_repo::enqueue_job(&state.db, job)
        .await
        .map_err(|e| AdminSyncError::Internal(crate::internal!(e)))?;

    Ok(Json(AdminSyncResponse {
        server_id: id,
        job_id: job.id,
        status: job.status,
    }))
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
    let id = req.param::<String>("id").unwrap_or_default();
    let job = jobs_repo::retry_job(&state.db, &id)
        .await
        .map_err(|e| match e {
            JobError::NotFound => AdminJobError::NotFound,
            e => AdminJobError::Internal(crate::internal!(e)),
        })?;
    Ok(Json(job_to_dto(job)))
}
