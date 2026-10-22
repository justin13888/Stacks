use salvo::prelude::*;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use stacks_core::entity::watch_status;
use stacks_core::import_export::ConflictStrategy;
use stacks_core::import_export::export::{export_csv, export_json};
use stacks_core::import_export::models::{ExternalIds, ImportEntry};
use stacks_core::jobs::repo as jobs_repo;
use stacks_core::media::id::{MediaId, Provider};

use crate::app::AppState;
use crate::error::IntoInternal;
use crate::extractors::auth_user::require_user;

use super::dto::*;
use super::error::*;

/// Sources accepted by the import job.
const SUPPORTED_SOURCES: &[&str] = &["mal_xml", "mal_json", "tmdb_json"];

/// Begin an import: validate the source, enqueue a background `import` job,
/// and return its id. Poll [`get_import_status`] to track progress.
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

    if !SUPPORTED_SOURCES.contains(&body.source.as_str()) {
        return Err(StartImportError::UnsupportedFormat);
    }

    // Mint the job id up front so it can travel inside the payload: the
    // completion notification reports it back to the user as `import_id`, and
    // it is the same id the client polls with.
    let import_id = ulid::Ulid::new().to_string();
    let payload = serde_json::json!({
        "user_id": user.id,
        "source": body.source,
        "data_base64": body.data_base64,
        "conflict_strategy": parse_strategy(body.conflict_strategy.as_deref()),
        "import_id": import_id,
    });
    let mut job_model = jobs_repo::build_job(jobs_repo::JobType::Import, Some(payload), 3);
    job_model.id = sea_orm::ActiveValue::Set(import_id);
    let job = jobs_repo::enqueue_job(&state.db, job_model)
        .await
        .map_err(|e| StartImportError::Internal(crate::internal!(e)))?;

    Ok(Json(ImportStatusResponse {
        job_id: job.id,
        status: job.status,
        attempts: 0,
        error: None,
    }))
}

/// Poll the status of an import job by id.
#[salvo::oapi::endpoint(tags("Import / Export"), summary = "Get import status")]
pub async fn get_import_status(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<ImportStatusResponse>, GetImportStatusError> {
    let _user = require_user(depot).expect("auth middleware populates depot");
    let state = depot.obtain::<AppState>().unwrap().clone();
    let job_id = req.param::<String>("job_id").unwrap_or_default();
    let job = jobs_repo::find_job_by_id(&state.db, &job_id)
        .await
        .map_err(|e| GetImportStatusError::Internal(crate::internal!(e)))?
        .ok_or(GetImportStatusError::NotFound)?;
    Ok(Json(ImportStatusResponse {
        job_id: job.id,
        status: job.status,
        attempts: job.attempts.max(0) as u32,
        error: job.error,
    }))
}

/// Export the user's watch data as a JSON or CSV file download.
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

    let rows = watch_status::Entity::find()
        .filter(watch_status::Column::UserId.eq(&user.id))
        .all(&state.db)
        .await
        .map_err(|e| ExportError::Internal(crate::internal!(e)))?;
    let entries: Vec<ImportEntry> = rows.into_iter().map(to_import_entry).collect();

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

fn parse_strategy(raw: Option<&str>) -> ConflictStrategy {
    match raw.unwrap_or("skip") {
        "overwrite" => ConflictStrategy::Overwrite,
        "merge" => ConflictStrategy::Merge,
        _ => ConflictStrategy::Skip,
    }
}

/// Convert a stored `watch_status` row into a portable [`ImportEntry`].
fn to_import_entry(w: watch_status::Model) -> ImportEntry {
    let mut external_ids = ExternalIds::default();
    let mut source = String::new();
    if let Ok(id) = MediaId::parse(&w.cameo_media_id) {
        match id.provider {
            Provider::Tmdb => {
                external_ids.tmdb_id = Some(id.numeric.to_string());
                source = format!("tmdb:{}", id.kind);
            }
            Provider::AniList => {
                external_ids.anilist_id = Some(id.numeric.to_string());
                source = format!("anilist:{}", id.kind);
            }
        }
    }
    ImportEntry {
        external_ids,
        status: Some(w.status),
        score: w.score,
        progress: None,
        started_at: w.started_at.map(|t| t.with_timezone(&chrono::Utc)),
        completed_at: w.completed_at.map(|t| t.with_timezone(&chrono::Utc)),
        source,
    }
}
