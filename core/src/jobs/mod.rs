pub mod error;
pub mod handlers;
pub mod repo;
pub mod runner;

use std::sync::Arc;

use crate::media::MetadataProvider;
use sea_orm::DatabaseConnection;

pub use error::JobError;
pub use runner::JobRunner;

/// Shared dependencies passed into every job handler: the database and the
/// metadata provider (for content lookups).
#[derive(Clone)]
pub struct JobContext {
    pub db: DatabaseConnection,
    pub metadata: Arc<dyn MetadataProvider>,
}
