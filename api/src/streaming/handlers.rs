use salvo::prelude::*;

use stacks_core::media::enrichment;
use stacks_core::streaming::StreamingService;

use crate::app::AppState;
use crate::extractors::auth_user::require_user;

use super::dto::*;
use super::error::*;

/// Check streaming availability for a media item across known servers.
#[salvo::oapi::endpoint(tags("Streaming"), summary = "Get media availability")]
pub async fn get_availability(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<AvailabilityResponse>, GetAvailabilityError> {
    let _user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let cameo_id = req.param::<String>("cameo_id").unwrap_or_default();
    let avail = enrichment::get_availability(&state.db, &cameo_id).await;
    Ok(Json(AvailabilityResponse {
        available: avail.available,
        servers: avail
            .servers
            .into_iter()
            .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
            .collect(),
    }))
}

/// List all configured streaming servers visible to the user.
#[salvo::oapi::endpoint(tags("Streaming"), summary = "List servers")]
pub async fn list_servers(
    depot: &mut Depot,
) -> Result<Json<ServerListResponse>, ListServersError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = StreamingService::new(state.db, &state.config.auth.secret_key);
    let servers = svc
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

/// Get health status of a specific streaming server.
#[salvo::oapi::endpoint(tags("Streaming"), summary = "Get server status")]
pub async fn get_server_status(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<ServerStatusResponse>, GetServerStatusError> {
    let _user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let id = req.param::<String>("id").unwrap_or_default();
    let svc = StreamingService::new(state.db, &state.config.auth.secret_key);
    let health = svc.health_check(&id).await.map_err(|e| match e {
        stacks_core::streaming::StreamingError::ServerNotFound => {
            GetServerStatusError::NotFound
        }
        e => GetServerStatusError::Internal(crate::internal!(e)),
    })?;
    Ok(Json(ServerStatusResponse {
        healthy: health.healthy,
        message: health.message.or(health.version),
    }))
}
