use sea_orm::DatabaseConnection;

use crate::entity::{server_library_item, streaming_server};

use super::StreamingError;

pub async fn create_server(
    db: &DatabaseConnection,
    model: streaming_server::ActiveModel,
) -> Result<streaming_server::Model, StreamingError> {
    let _ = (db, model);
    todo!()
}

pub async fn find_server_by_id(
    db: &DatabaseConnection,
    id: &str,
) -> Result<Option<streaming_server::Model>, StreamingError> {
    let _ = (db, id);
    todo!()
}

pub async fn find_servers_for_user(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<Vec<streaming_server::Model>, StreamingError> {
    let _ = (db, user_id);
    todo!()
}

pub async fn update_server(
    db: &DatabaseConnection,
    model: streaming_server::ActiveModel,
) -> Result<streaming_server::Model, StreamingError> {
    let _ = (db, model);
    todo!()
}

pub async fn delete_server(db: &DatabaseConnection, id: &str) -> Result<bool, StreamingError> {
    let _ = (db, id);
    todo!()
}

pub async fn upsert_library_item(
    db: &DatabaseConnection,
    model: server_library_item::ActiveModel,
) -> Result<server_library_item::Model, StreamingError> {
    let _ = (db, model);
    todo!()
}

pub async fn find_library_items_for_server(
    db: &DatabaseConnection,
    server_id: &str,
) -> Result<Vec<server_library_item::Model>, StreamingError> {
    let _ = (db, server_id);
    todo!()
}

pub async fn find_library_item_by_cameo_id(
    db: &DatabaseConnection,
    server_id: &str,
    cameo_media_id: &str,
) -> Result<Option<server_library_item::Model>, StreamingError> {
    let _ = (db, server_id, cameo_media_id);
    todo!()
}
