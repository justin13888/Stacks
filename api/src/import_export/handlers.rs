use base64::{Engine, engine::general_purpose::STANDARD as B64};
use chrono::Utc;
use salvo::prelude::*;
use sea_orm::ActiveValue::Set;
use ulid::Ulid;

use stacks_core::entity::job;
use stacks_core::import_export::export;
use stacks_core::import_export::models::ConflictStrategy;
use stacks_core::jobs::{JobStatus, JobType, repo as jobs_repo};

use crate::app::AppState;
use crate::extractors::auth_user::require_user;

use super::dto::*;
use super::error::*;

/// Begin an import job from an uploaded file.
///
/// Accepts JSON `{ "source": "tmdb_json"|"mal_xml"|"mal_json"|"stacks_json",
/// "data_base64": "...", "conflict_strategy": "skip"|"overwrite"|"merge" }`.
#[salvo::oapi::endpoint(tags("Import / Export"), summary = "Start import")]
pub async fn start_import(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<ImportStatusResponse>, StartImportError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();

    #[derive(serde::Deserialize)]
    struct StartImportRequest {
        source: String,
        data_base64: String,
        #[serde(default)]
        conflict_strategy: ConflictStrategy,
    }

    let body: StartImportRequest = req
        .parse_json()
        .await
        .map_err(|e| StartImportError::Internal(crate::internal!(e)))?;

    if !matches!(
        body.source.as_str(),
        "tmdb_json" | "tmdb" | "mal_xml" | "mal" | "mal_json" | "stacks_json"
    ) {
        return Err(StartImportError::UnsupportedFormat);
    }

    let payload = serde_json::json!({
        "user_id": user.id,
        "source": body.source,
        "data_base64": body.data_base64,
        "conflict_strategy": body.conflict_strategy,
    });

    let model = job::ActiveModel {
        id: Set(Ulid::new().to_string()),
        job_type: Set(JobType::Import.as_str().to_string()),
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
        .map_err(|e| StartImportError::Internal(crate::internal!(e)))?;

    Ok(Json(ImportStatusResponse {
        job_id: inserted.id,
        status: inserted.status,
        progress: None,
        report: None,
    }))
}

/// Poll the status of an in-progress or completed import job.
#[salvo::oapi::endpoint(tags("Import / Export"), summary = "Get import status")]
pub async fn get_import_status(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<ImportStatusResponse>, GetImportStatusError> {
    let _user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let job_id = req.param::<String>("job_id").unwrap_or_default();
    let job = jobs_repo::find_job_by_id(&state.db, &job_id)
        .await
        .map_err(|e| GetImportStatusError::Internal(crate::internal!(e)))?
        .ok_or(GetImportStatusError::NotFound)?;
    Ok(Json(ImportStatusResponse {
        job_id: job.id,
        status: job.status,
        progress: None,
        report: job.error.map(|e| serde_json::json!({ "error": e })),
    }))
}

/// Export the user's watch data in the requested format.
#[salvo::oapi::endpoint(tags("Import / Export"), summary = "Export user data")]
pub async fn export_data(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) -> Result<(), ExportError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let format = req.query::<String>("format").unwrap_or_else(|| "json".into());

    let entries = export::build_user_export(&state.db, &user.id)
        .await
        .map_err(|e| ExportError::Internal(crate::internal!(e)))?;

    let (bytes, content_type, filename) = match format.as_str() {
        "json" => {
            let bytes = export::export_json(&entries)
                .map_err(|e| ExportError::Internal(crate::internal!(e)))?;
            (bytes, "application/json", "stacks-export.json")
        }
        "csv" => {
            let bytes = export::export_csv(&entries)
                .map_err(|e| ExportError::Internal(crate::internal!(e)))?;
            (bytes, "text/csv", "stacks-export.csv")
        }
        _ => return Err(ExportError::UnsupportedFormat),
    };

    res.headers_mut()
        .insert("content-type", content_type.parse().unwrap());
    res.headers_mut().insert(
        "content-disposition",
        format!("attachment; filename=\"{filename}\"").parse().unwrap(),
    );
    res.write_body(bytes).ok();
    Ok(())
}

// Re-export base64 type so the dto module can encode upstream callers if
// they ever need a non-multipart upload path.
#[allow(dead_code)]
fn _b64_assert(_: &[u8]) -> String {
    B64.encode([0u8])
}
