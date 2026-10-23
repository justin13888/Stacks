use thiserror::Error;

#[derive(Debug, Error)]
pub enum ListError {
    #[error("list not found")]
    NotFound,
    #[error("not the owner of this list")]
    NotOwner,
    #[error("list name must not be empty")]
    EmptyName,
    #[error("this operation requires a smart list")]
    NotSmart,
    #[error("invalid smart-list filter query: {0}")]
    InvalidFilter(String),
    #[error("already in this list")]
    AlreadyInList,
    #[error(transparent)]
    Position(#[from] super::fractional_index::FractionalIndexError),
    #[error(transparent)]
    Media(#[from] crate::media::MediaError),
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}
