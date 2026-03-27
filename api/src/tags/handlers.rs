use salvo::prelude::*;

use super::dto::*;
use super::error::*;

/// List all tags belonging to the authenticated user.
#[salvo::oapi::endpoint(tags("Tags"), summary = "List tags")]
pub async fn list_tags(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<Vec<TagResponse>>, DeleteTagError> {
    todo!()
}

/// Create a new tag.
#[salvo::oapi::endpoint(tags("Tags"), summary = "Create tag")]
pub async fn create_tag(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<TagResponse>, CreateTagError> {
    todo!()
}

/// Delete a tag by ID.
#[salvo::oapi::endpoint(tags("Tags"), summary = "Delete tag")]
pub async fn delete_tag(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<()>, DeleteTagError> {
    todo!()
}

/// Replace the full set of tags on a media item.
#[salvo::oapi::endpoint(tags("Tags"), summary = "Set media tags")]
pub async fn set_media_tags(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<Vec<TagResponse>>, SetMediaTagsError> {
    todo!()
}
