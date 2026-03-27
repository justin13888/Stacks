use salvo::prelude::*;

use super::dto::*;
use super::error::*;

/// Check streaming availability for a media item across known servers.
#[salvo::oapi::endpoint(tags("Streaming"), summary = "Get media availability")]
pub async fn get_availability(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<AvailabilityResponse>, GetAvailabilityError> {
    todo!()
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
