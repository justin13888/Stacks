pub mod error;
pub mod handlers;
pub mod repo;
pub mod runner;

use std::sync::Arc;

use crate::media::MediaClient;
use sea_orm::DatabaseConnection;

pub use error::JobError;
pub use runner::JobRunner;

/// Shared dependencies passed into every job handler.
///
/// Carries the database connection, the cameo client (for content lookups),
/// and the AES key for decrypting streaming-server credentials.
#[derive(Clone)]
pub struct JobContext {
    pub db: DatabaseConnection,
    pub cameo: Arc<MediaClient>,
    pub encryption_key: [u8; 32],
}
