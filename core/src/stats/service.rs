use sea_orm::DatabaseConnection;

use super::models::UserStats;
use super::repo::StatsError;

pub struct StatsService {
    pub db: DatabaseConnection,
}

impl StatsService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn get_user_stats(&self, user_id: &str) -> Result<UserStats, StatsError> {
        let _ = user_id;
        todo!()
    }
}
