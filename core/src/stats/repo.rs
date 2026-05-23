use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, Order, QueryFilter, QueryOrder};
use thiserror::Error;

use crate::entity::{watch_history, watch_status};

#[derive(Debug, Error)]
pub enum StatsError {
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}

/// All of a user's watch-status rows, most recently updated first.
pub async fn watch_statuses(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<Vec<watch_status::Model>, StatsError> {
    watch_status::Entity::find()
        .filter(watch_status::Column::UserId.eq(user_id))
        .order_by(watch_status::Column::UpdatedAt, Order::Desc)
        .all(db)
        .await
        .map_err(StatsError::Db)
}

/// All of a user's watch-history events.
pub async fn watch_history_events(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<Vec<watch_history::Model>, StatsError> {
    watch_history::Entity::find()
        .filter(watch_history::Column::UserId.eq(user_id))
        .all(db)
        .await
        .map_err(StatsError::Db)
}
