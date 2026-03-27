use salvo::prelude::*;

use super::dto::*;
use super::error::*;

/// List all blocked media for the authenticated user.
#[salvo::oapi::endpoint(tags("Blocklist"), summary = "List blocklist")]
pub async fn list_blocklist(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<BlocklistResponse>, AddBlockError> {
    todo!()
}

/// Add a media item to the user's blocklist.
#[salvo::oapi::endpoint(tags("Blocklist"), summary = "Add to blocklist")]
pub async fn add_to_blocklist(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<BlocklistEntryResponse>, AddBlockError> {
    todo!()
}

/// Remove a media item from the user's blocklist.
#[salvo::oapi::endpoint(tags("Blocklist"), summary = "Remove from blocklist")]
pub async fn remove_from_blocklist(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<()>, RemoveBlockError> {
    todo!()
}
