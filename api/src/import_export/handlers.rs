use base64::Engine;
use salvo::prelude::*;
use stacks_core::entity::enums::{ConflictStrategy, ImportSource};
use stacks_core::import_export::ImportExportError;
use stacks_core::import_export::export::{export_csv, export_json, library_rows};
use stacks_core::import_export::import::pipeline;

use crate::app::AppState;
use crate::error::IntoInternal;
use crate::extractors::auth_user::require_user;
use crate::extractors::uuid_param;

use super::dto::*;
use super::error::*;

fn status_dto(i: stacks_core::entity::import::Model) -> ImportStatusResponse {
    ImportStatusResponse {
        job_id: i.id.to_string(),
        status: i.status.to_string(),
        attempts: 0,
        error: i.error,
    }
}

/// Begin an import: the file is stored (up to the configured size cap) and a
/// background job is queued, in one transaction. Poll [`get_import_status`]
/// with the returned id.
#[salvo::oapi::endpoint(tags("Import / Export"), summary = "Start import")]
pub async fn start_import(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<ImportStatusResponse>, StartImportError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let body: StartImportRequest = req.parse_json().await.into_internal()?;

    let source = ImportSource::parse(&body.source).ok_or(StartImportError::UnsupportedFormat)?;
    let strategy = body
        .conflict_strategy
        .as_deref()
        .and_then(ConflictStrategy::parse)
        .unwrap_or_default();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(body.data_base64.as_bytes())
        .map_err(|_| StartImportError::UnsupportedFormat)?;

    let import = pipeline::submit(
        &state.db,
        user.id,
        source,
        None,
        bytes,
        strategy,
        state.config.imports.max_file_bytes,
    )
    .await
    .map_err(|e| match e {
        ImportExportError::TooLarge { .. } => StartImportError::UnsupportedFormat,
        e => StartImportError::Internal(crate::internal!(e)),
    })?;
    Ok(Json(status_dto(import)))
}

/// Poll the status of an import by id.
#[salvo::oapi::endpoint(tags("Import / Export"), summary = "Get import status")]
pub async fn get_import_status(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<ImportStatusResponse>, GetImportStatusError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let id = uuid_param(req, "job_id").ok_or(GetImportStatusError::NotFound)?;
    let import = pipeline::find_for_user(&state.db, user.id, id)
        .await
        .map_err(|e| GetImportStatusError::Internal(crate::internal!(e)))?
        .ok_or(GetImportStatusError::NotFound)?;
    Ok(Json(status_dto(import)))
}

/// Export the user's library as a JSON or CSV file download.
#[salvo::oapi::endpoint(tags("Import / Export"), summary = "Export user data")]
pub async fn export(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) -> Result<(), ExportError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let format = req
        .query::<String>("format")
        .unwrap_or_else(|| "json".to_string());

    let entries = library_rows(&state.db, user.id)
        .await
        .map_err(|e| ExportError::Internal(crate::internal!(e)))?;

    let (bytes, content_type, filename) = match format.as_str() {
        "csv" => (
            export_csv(&entries).map_err(|e| ExportError::Internal(crate::internal!(e)))?,
            "text/csv",
            "stacks-export.csv",
        ),
        "json" => (
            export_json(&entries).map_err(|e| ExportError::Internal(crate::internal!(e)))?,
            "application/json",
            "stacks-export.json",
        ),
        _ => return Err(ExportError::UnsupportedFormat),
    };

    let _ = res.add_header("content-type", content_type, true);
    let _ = res.add_header(
        "content-disposition",
        format!("attachment; filename=\"{filename}\""),
        true,
    );
    let _ = res.write_body(bytes);
    Ok(())
}
