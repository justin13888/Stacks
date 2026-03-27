use sea_orm::DatabaseConnection;

use crate::import_export::error::ImportExportError;
use crate::import_export::models::{ConflictStrategy, ImportEntry, ImportReport};

pub struct ImportPipeline {
    pub db: DatabaseConnection,
}

impl ImportPipeline {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Run a full import for a user given a list of pre-parsed `ImportEntry` values.
    pub async fn run(
        &self,
        user_id: &str,
        entries: Vec<ImportEntry>,
        conflict_strategy: ConflictStrategy,
    ) -> Result<ImportReport, ImportExportError> {
        let _ = (user_id, entries, conflict_strategy);
        todo!()
    }

    /// Resolve the canonical Cameo media ID for a single entry using its external IDs.
    pub async fn resolve_cameo_id(
        &self,
        entry: &ImportEntry,
    ) -> Result<Option<String>, ImportExportError> {
        let _ = entry;
        todo!()
    }
}
