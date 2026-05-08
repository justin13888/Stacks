use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, FromQueryResult, QueryFilter,
    QuerySelect, Statement,
};
use thiserror::Error;

use crate::entity::{episode_watch_status, watch_history, watch_status};

use super::models::UserStats;

#[derive(Debug, Error)]
pub enum StatsError {
    #[error("stats not available")]
    Unavailable,
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}

/// Compute aggregated stats for a user. Single transaction for consistency.
pub async fn compute_user_stats(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<UserStats, StatsError> {
    let total = get_total_watch_time_minutes(db, user_id).await?;
    let yearly = get_yearly_counts(db, user_id).await?;
    let monthly = get_monthly_heatmap(db, user_id).await?;
    Ok(UserStats {
        total_watch_time_minutes: total,
        // Genre breakdown requires Cameo metadata; populated by the service
        // layer when it has access to the cameo client.
        genre_breakdown: Vec::new(),
        yearly_counts: yearly,
        monthly_heatmap: monthly,
    })
}

/// Estimated total watch time. Sums the maximum `position_seconds` value per
/// `(user_id, cameo_media_id, cameo_episode_id)` tuple from `watch_history`,
/// converted to minutes. This avoids double-counting repeat plays of the
/// same episode while remaining a single SQL round-trip.
pub async fn get_total_watch_time_minutes(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<i64, StatsError> {
    #[derive(FromQueryResult)]
    struct Row {
        total_seconds: Option<i64>,
    }

    let backend = db.get_database_backend();
    let stmt = Statement::from_sql_and_values(
        backend,
        r#"
        SELECT COALESCE(SUM(max_pos), 0)::BIGINT AS total_seconds
        FROM (
            SELECT MAX(position_seconds) AS max_pos
            FROM watch_history
            WHERE user_id = $1 AND position_seconds IS NOT NULL
            GROUP BY cameo_media_id, COALESCE(cameo_episode_id, '')
        ) sub
        "#,
        [user_id.into()],
    );
    let row = Row::find_by_statement(stmt).one(db).await?;
    Ok(row
        .and_then(|r| r.total_seconds)
        .map(|s| s / 60)
        .unwrap_or(0))
}

/// Yearly counts: number of titles `completed_at`-stamped in each calendar year.
pub async fn get_yearly_counts(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<Vec<(i32, u32)>, StatsError> {
    #[derive(FromQueryResult)]
    struct Row {
        year: i32,
        count: i64,
    }

    let backend = db.get_database_backend();
    let stmt = Statement::from_sql_and_values(
        backend,
        r#"
        SELECT EXTRACT(YEAR FROM completed_at)::INT AS year, COUNT(*)::BIGINT AS count
        FROM watch_status
        WHERE user_id = $1 AND completed_at IS NOT NULL
        GROUP BY year
        ORDER BY year
        "#,
        [user_id.into()],
    );
    let rows = Row::find_by_statement(stmt).all(db).await?;
    Ok(rows.into_iter().map(|r| (r.year, r.count as u32)).collect())
}

/// Monthly heatmap: episode/movie completions per `YYYY-MM`.
pub async fn get_monthly_heatmap(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<Vec<(String, u32)>, StatsError> {
    #[derive(FromQueryResult)]
    struct Row {
        month: String,
        count: i64,
    }

    let backend = db.get_database_backend();
    let stmt = Statement::from_sql_and_values(
        backend,
        r#"
        SELECT TO_CHAR(timestamp, 'YYYY-MM') AS month, COUNT(*)::BIGINT AS count
        FROM watch_history
        WHERE user_id = $1 AND event = 'complete'
        GROUP BY month
        ORDER BY month
        "#,
        [user_id.into()],
    );
    let rows = Row::find_by_statement(stmt).all(db).await?;
    Ok(rows.into_iter().map(|r| (r.month, r.count as u32)).collect())
}

/// Return the distinct cameo_media_ids the user has any data for. Used by
/// the service layer to fetch genre metadata via Cameo.
pub async fn distinct_user_media_ids(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<Vec<String>, StatsError> {
    let mut ids: Vec<String> = watch_status::Entity::find()
        .filter(watch_status::Column::UserId.eq(user_id))
        .select_only()
        .column(watch_status::Column::CameoMediaId)
        .into_tuple()
        .all(db)
        .await?;
    let extra: Vec<String> = watch_history::Entity::find()
        .filter(watch_history::Column::UserId.eq(user_id))
        .select_only()
        .column(watch_history::Column::CameoMediaId)
        .into_tuple()
        .all(db)
        .await?;
    let extra_eps: Vec<String> = episode_watch_status::Entity::find()
        .filter(episode_watch_status::Column::UserId.eq(user_id))
        .select_only()
        .column(episode_watch_status::Column::CameoMediaId)
        .into_tuple()
        .all(db)
        .await?;
    ids.extend(extra);
    ids.extend(extra_eps);
    ids.sort();
    ids.dedup();
    Ok(ids)
}
