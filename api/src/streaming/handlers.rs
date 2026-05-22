use salvo::prelude::*;
use stacks_core::crypto;
use stacks_core::media::{MediaId, enrichment};
use stacks_core::streaming::{StreamingError, StreamingService};

use crate::app::AppState;

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

/// List the streaming servers visible to the authenticated user.
#[salvo::oapi::endpoint(tags("Streaming"), summary = "List servers")]
pub async fn list_servers(depot: &mut Depot) -> Result<Json<ServerListResponse>, ListServersError> {
    let user = crate::extractors::auth_user::require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let servers = streaming_service(&state)
        .list_servers(&user.id)
        .await
        .map_err(|e| ListServersError::Internal(crate::internal!(e)))?;
    Ok(Json(ServerListResponse {
        servers: servers
            .into_iter()
            .map(|s| ServerResponse {
                id: s.id,
                name: s.name,
                server_type: s.server_type,
                enabled: s.enabled,
                last_synced_at: s.last_synced_at.map(|t| t.to_rfc3339()),
            })
            .collect(),
    }))
}

/// Get the live health status of a specific streaming server.
#[salvo::oapi::endpoint(tags("Streaming"), summary = "Get server status")]
pub async fn get_server_status(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<ServerStatusResponse>, GetServerStatusError> {
    let state = depot.obtain::<AppState>().unwrap().clone();
    let id = req.param::<String>("id").unwrap_or_default();
    let health = streaming_service(&state)
        .health_check(&id)
        .await
        .map_err(|e| match e {
            StreamingError::ServerNotFound => GetServerStatusError::NotFound,
            e => GetServerStatusError::Internal(crate::internal!(e)),
        })?;
    Ok(Json(ServerStatusResponse {
        healthy: health.healthy,
        message: health.message,
    }))
}
