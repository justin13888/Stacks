use thiserror::Error;

#[derive(Debug, Error)]
pub enum ListError {
    #[error("list not found")]
    NotFound,
    #[error("not the owner of this list")]
    NotOwner,
    #[error("list name must not be empty")]
    EmptyName,
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}
