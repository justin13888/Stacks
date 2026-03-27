use sea_orm::DatabaseConnection;
use thiserror::Error;

use super::models::UserStats;

#[derive(Debug, Error)]
pub enum StatsError {
    #[error("stats not available")]
    Unavailable,
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}

pub async fn compute_user_stats(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<UserStats, StatsError> {
    let _ = (db, user_id);
    todo!()
}

pub async fn get_total_watch_time_minutes(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<i64, StatsError> {
    let _ = (db, user_id);
    todo!()
}

pub async fn get_genre_breakdown(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<Vec<(String, u32)>, StatsError> {
    let _ = (db, user_id);
    todo!()
}

pub async fn get_yearly_counts(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<Vec<(i32, u32)>, StatsError> {
    let _ = (db, user_id);
    todo!()
}

pub async fn get_monthly_heatmap(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<Vec<(String, u32)>, StatsError> {
    let _ = (db, user_id);
    todo!()
}
