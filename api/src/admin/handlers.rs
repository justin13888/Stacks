use chrono::Utc;
use salvo::prelude::*;
use sea_orm::ActiveValue::Set;
use ulid::Ulid;

use stacks_core::entity::job;
use stacks_core::jobs::{JobStatus, JobType, repo as jobs_repo};
use stacks_core::streaming::StreamingService;

use crate::app::AppState;
use crate::error::IntoInternal;

use super::dto::*;
use super::error::*;

fn server_to_dto(s: stacks_core::entity::streaming_server::Model) -> AdminServerResponse {
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
    let svc = StreamingService::new(state.db, &state.config.auth.secret_key);
    let servers = svc
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
    let svc = StreamingService::new(state.db, &state.config.auth.secret_key);
    let server = svc
        .register_server(
            None,
            body.name,
            body.server_type,
            body.base_url,
            body.api_key,
            body.webhook_secret,
        )
        .await
        .map_err(|e| AdminCreateServerError::Internal(crate::internal!(e)))?;
    Ok(Json(server_to_dto(server)))
}

#[derive(Debug, serde::Deserialize, salvo::oapi::ToSchema)]
pub struct AdminUpdateServerRequest {
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub webhook_secret: Option<String>,
    pub enabled: Option<bool>,
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
    let svc = StreamingService::new(state.db, &state.config.auth.secret_key);
    let server = svc
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
            stacks_core::streaming::StreamingError::ServerNotFound => {
                AdminDeleteServerError::NotFound
            }
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
    let svc = StreamingService::new(state.db, &state.config.auth.secret_key);
    svc.delete_server(&id).await.map_err(|e| match e {
        stacks_core::streaming::StreamingError::ServerNotFound => AdminDeleteServerError::NotFound,
        e => AdminDeleteServerError::Internal(crate::internal!(e)),
    })?;
    Ok(Json(()))
}

/// Trigger a manual metadata sync for a server (admin).
#[salvo::oapi::endpoint(tags("Admin"), summary = "Sync server (admin)")]
pub async fn sync_server(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<serde_json::Value>, AdminSyncError> {
    let state = depot.obtain::<AppState>().unwrap().clone();
    let id = req.param::<String>("id").unwrap_or_default();

    // Confirm the server exists before enqueueing.
    let svc = StreamingService::new(state.db.clone(), &state.config.auth.secret_key);
    svc.get_server(&id).await.map_err(|_| AdminSyncError::NotFound)?;

    let payload = serde_json::json!({ "server_id": id });
    let model = job::ActiveModel {
        id: Set(Ulid::new().to_string()),
        job_type: Set(JobType::BeamSync.as_str().to_string()),
        payload: Set(Some(payload)),
        status: Set(JobStatus::Pending.as_str().to_string()),
        attempts: Set(0),
        max_attempts: Set(3),
        run_at: Set(Utc::now().into()),
        started_at: Set(None),
        completed_at: Set(None),
        error: Set(None),
        created_at: Set(Utc::now().into()),
    };
    let inserted = jobs_repo::enqueue_job(&state.db, model)
        .await
        .map_err(|e| AdminSyncError::Internal(crate::internal!(e)))?;

    Ok(Json(serde_json::json!({
        "job_id": inserted.id,
        "status": inserted.status,
    })))
}

/// List background jobs (admin).
#[salvo::oapi::endpoint(tags("Admin"), summary = "List jobs (admin)")]
pub async fn list_jobs(depot: &mut Depot) -> Result<Json<AdminJobListResponse>, AdminJobError> {
    let state = depot.obtain::<AppState>().unwrap().clone();
    let jobs = jobs_repo::list_recent_jobs(&state.db, 100)
        .await
        .map_err(|e| AdminJobError::Internal(crate::internal!(e)))?;
    Ok(Json(AdminJobListResponse {
        jobs: jobs.into_iter().map(job_to_dto).collect(),
    }))
}

/// Retry a failed background job (admin).
#[salvo::oapi::endpoint(tags("Admin"), summary = "Retry job (admin)")]
pub async fn retry_job(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<AdminJobResponse>, AdminJobError> {
    let state = depot.obtain::<AppState>().unwrap().clone();
    let id = req.param::<String>("id").unwrap_or_default();
    let job = jobs_repo::find_job_by_id(&state.db, &id)
        .await
        .map_err(|e| AdminJobError::Internal(crate::internal!(e)))?
        .ok_or(AdminJobError::NotFound)?;
    // Reset to pending; runner will pick it up on next tick.
    let updated = jobs_repo::update_job_status(&state.db, &job.id, JobStatus::Pending, None)
        .await
        .map_err(|e| AdminJobError::Internal(crate::internal!(e)))?;
    Ok(Json(job_to_dto(updated)))
}
