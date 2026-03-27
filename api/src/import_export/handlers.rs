use salvo::prelude::*;

use super::dto::*;
use super::error::*;

/// Begin an import job from an uploaded file.
#[salvo::oapi::endpoint(tags("Import / Export"), summary = "Start import")]
pub async fn start_import(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<ImportStatusResponse>, StartImportError> {
    todo!()
}

/// Poll the status of an in-progress or completed import job.
#[salvo::oapi::endpoint(tags("Import / Export"), summary = "Get import status")]
pub async fn get_import_status(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<ImportStatusResponse>, GetImportStatusError> {
    todo!()
}

/// Export the user's watch data in the requested format.
#[salvo::oapi::endpoint(tags("Import / Export"), summary = "Export user data")]
pub async fn export(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<serde_json::Value>, ExportError> {
    todo!()
}
