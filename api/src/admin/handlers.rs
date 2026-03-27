use salvo::prelude::*;

use super::dto::*;
use super::error::*;

/// List all registered streaming servers (admin).
#[salvo::oapi::endpoint(tags("Admin"), summary = "List servers (admin)")]
pub async fn list_servers(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<Vec<AdminServerResponse>>, AdminCreateServerError> {
    todo!()
}

/// Register a new streaming server (admin).
#[salvo::oapi::endpoint(tags("Admin"), summary = "Create server (admin)")]
pub async fn create_server(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<AdminServerResponse>, AdminCreateServerError> {
    todo!()
}

/// Update a streaming server's configuration (admin).
#[salvo::oapi::endpoint(tags("Admin"), summary = "Update server (admin)")]
pub async fn update_server(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<AdminServerResponse>, AdminDeleteServerError> {
    todo!()
}

/// Delete a streaming server (admin).
#[salvo::oapi::endpoint(tags("Admin"), summary = "Delete server (admin)")]
pub async fn delete_server(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<()>, AdminDeleteServerError> {
    todo!()
}

/// Trigger a manual metadata sync for a server (admin).
#[salvo::oapi::endpoint(tags("Admin"), summary = "Sync server (admin)")]
pub async fn sync_server(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<serde_json::Value>, AdminSyncError> {
    todo!()
}

/// List background jobs (admin).
#[salvo::oapi::endpoint(tags("Admin"), summary = "List jobs (admin)")]
pub async fn list_jobs(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<AdminJobListResponse>, AdminJobError> {
    todo!()
}

/// Retry a failed background job (admin).
#[salvo::oapi::endpoint(tags("Admin"), summary = "Retry job (admin)")]
pub async fn retry_job(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<AdminJobResponse>, AdminJobError> {
    todo!()
}
