pub mod error;
pub mod handlers;
pub mod repo;
pub mod runner;

use std::sync::Arc;

use crate::media::MediaClient;
use sea_orm::DatabaseConnection;

pub use error::JobError;
pub use runner::JobRunner;

/// Shared dependencies passed into every job handler: the database and the
/// cameo client (for content lookups).
#[derive(Clone)]
pub struct JobContext {
    pub db: DatabaseConnection,
    pub cameo: Arc<MediaClient>,
}
