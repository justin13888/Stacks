use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, Order, QueryFilter, QueryOrder,
    sea_query::OnConflict,
};

use crate::entity::{server_library_item, streaming_server};

use super::StreamingError;

pub async fn create_server(
    db: &DatabaseConnection,
    model: streaming_server::ActiveModel,
) -> Result<streaming_server::Model, StreamingError> {
    model.insert(db).await.map_err(StreamingError::Db)
}

pub async fn find_server_by_id(
    db: &DatabaseConnection,
    id: &str,
) -> Result<Option<streaming_server::Model>, StreamingError> {
    streaming_server::Entity::find_by_id(id)
        .one(db)
        .await
        .map_err(StreamingError::Db)
}

/// Every registered server, regardless of owner — for the admin surface.
pub async fn find_all_servers(
    db: &DatabaseConnection,
) -> Result<Vec<streaming_server::Model>, StreamingError> {
    streaming_server::Entity::find()
        .order_by(streaming_server::Column::CreatedAt, Order::Asc)
        .all(db)
        .await
        .map_err(StreamingError::Db)
}

/// Servers visible to a user: their own plus any global (unowned) servers.
pub async fn find_servers_for_user(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<Vec<streaming_server::Model>, StreamingError> {
    streaming_server::Entity::find()
        .filter(
            streaming_server::Column::UserId
                .eq(user_id)
                .or(streaming_server::Column::UserId.is_null()),
        )
        .order_by(streaming_server::Column::CreatedAt, Order::Asc)
        .all(db)
        .await
        .map_err(StreamingError::Db)
}

pub async fn update_server(
    db: &DatabaseConnection,
    model: streaming_server::ActiveModel,
) -> Result<streaming_server::Model, StreamingError> {
    model.update(db).await.map_err(StreamingError::Db)
}

pub async fn delete_server(db: &DatabaseConnection, id: &str) -> Result<bool, StreamingError> {
    let result = streaming_server::Entity::delete_by_id(id)
        .exec(db)
        .await
        .map_err(StreamingError::Db)?;
    Ok(result.rows_affected > 0)
}

/// Insert or update a library item, keyed on `(server_id, server_internal_id)`
/// — the unique index established by migration `m013`.
pub async fn upsert_library_item(
    db: &DatabaseConnection,
    model: server_library_item::ActiveModel,
) -> Result<server_library_item::Model, StreamingError> {
    server_library_item::Entity::insert(model)
        .on_conflict(
            OnConflict::columns([
                server_library_item::Column::ServerId,
                server_library_item::Column::ServerInternalId,
            ])
            .update_columns([
                server_library_item::Column::CameoMediaId,
                server_library_item::Column::Title,
                server_library_item::Column::Quality,
                server_library_item::Column::DeepLink,
                server_library_item::Column::PlaybackLink,
                server_library_item::Column::ExternalIds,
                server_library_item::Column::LastSeenAt,
            ])
            .to_owned(),
        )
        .exec_with_returning(db)
        .await
        .map_err(StreamingError::Db)
}

pub async fn find_library_items_for_server(
    db: &DatabaseConnection,
    server_id: &str,
) -> Result<Vec<server_library_item::Model>, StreamingError> {
    server_library_item::Entity::find()
        .filter(server_library_item::Column::ServerId.eq(server_id))
        .order_by(server_library_item::Column::Title, Order::Asc)
        .all(db)
        .await
        .map_err(StreamingError::Db)
}

pub async fn find_library_item_by_cameo_id(
    db: &DatabaseConnection,
    server_id: &str,
    cameo_media_id: &str,
) -> Result<Option<server_library_item::Model>, StreamingError> {
    server_library_item::Entity::find()
        .filter(server_library_item::Column::ServerId.eq(server_id))
        .filter(server_library_item::Column::CameoMediaId.eq(cameo_media_id))
        .one(db)
        .await
        .map_err(StreamingError::Db)
}
