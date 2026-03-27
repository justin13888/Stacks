use sea_orm::DatabaseConnection;

use super::JobError;

/// Core job runner responsible for polling pending jobs and dispatching them
/// to the appropriate handler.
pub struct JobRunner {
    pub db: DatabaseConnection,
}

impl JobRunner {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Poll for pending jobs and execute them, respecting max_attempts.
    pub async fn run(&self) -> Result<(), JobError> {
        todo!()
    }

    /// Execute a single job by ID.
    pub async fn run_job(&self, job_id: &str) -> Result<(), JobError> {
        let _ = job_id;
        todo!()
    }
}
