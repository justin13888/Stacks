use salvo::prelude::*;
use stacks_core::media::{MediaId, enrichment};

use crate::app::AppState;

use super::dto::*;
use super::error::*;

/// Check streaming availability for a media item across the user's servers.
#[salvo::oapi::endpoint(tags("Streaming"), summary = "Get media availability")]
pub async fn get_availability(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<AvailabilityResponse>, GetAvailabilityError> {
    let state = depot.obtain::<AppState>().unwrap().clone();
    let raw = req.param::<String>("cameo_id").unwrap_or_default();
    let media_id = match MediaId::parse(&raw) {
        Ok(id) => id,
        // A malformed id cannot match anything — report it as unavailable.
        Err(_) => {
            return Ok(Json(AvailabilityResponse {
                available: false,
                servers: Vec::new(),
            }));
        }
    };
    let info = enrichment::availability_for(&state.db, &media_id)
        .await
        .map_err(|e| GetAvailabilityError::Internal(crate::internal!(e)))?;
    Ok(Json(info.into()))
}

/// List all configured streaming servers.
#[salvo::oapi::endpoint(tags("Streaming"), summary = "List servers")]
pub async fn list_servers(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<ServerListResponse>, ListServersError> {
    todo!()
}

/// Get health status of a specific streaming server.
#[salvo::oapi::endpoint(tags("Streaming"), summary = "Get server status")]
pub async fn get_server_status(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<ServerStatusResponse>, GetServerStatusError> {
    todo!()
}
