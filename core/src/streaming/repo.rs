use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
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
        .all(db)
        .await
        .map_err(StreamingError::Db)
}

pub async fn list_all_servers(
    db: &DatabaseConnection,
) -> Result<Vec<streaming_server::Model>, StreamingError> {
    streaming_server::Entity::find()
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
    let res = streaming_server::Entity::delete_by_id(id)
        .exec(db)
        .await
        .map_err(StreamingError::Db)?;
    Ok(res.rows_affected > 0)
}

pub async fn upsert_library_item(
    db: &DatabaseConnection,
    model: server_library_item::ActiveModel,
) -> Result<server_library_item::Model, StreamingError> {
    server_library_item::Entity::insert(model)
        .on_conflict(
            OnConflict::column(server_library_item::Column::Id)
                .update_columns([
                    server_library_item::Column::ServerId,
                    server_library_item::Column::CameoMediaId,
                    server_library_item::Column::ServerInternalId,
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

pub async fn find_library_items_by_cameo_id(
    db: &DatabaseConnection,
    cameo_media_id: &str,
) -> Result<Vec<server_library_item::Model>, StreamingError> {
    server_library_item::Entity::find()
        .filter(server_library_item::Column::CameoMediaId.eq(cameo_media_id))
        .all(db)
        .await
        .map_err(StreamingError::Db)
}
