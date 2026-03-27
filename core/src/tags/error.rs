use thiserror::Error;

#[derive(Debug, Error)]
pub enum TagError {
    #[error("tag not found")]
    NotFound,
    #[error("a tag with that name already exists")]
    NameTaken,
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}
