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
    #[error("a smart list's titles come from its filter and cannot be added by hand")]
    NotManual,
    #[error("invalid smart-list filter query: {0}")]
    InvalidFilter(String),
    #[error("already in this list")]
    AlreadyInList,
    #[error("the title is not in this list")]
    ItemNotFound,
    /// The item a placement is relative to is not in the list.
    #[error("the title to place the item next to is not in this list")]
    AnchorNotFound,
    /// The placement names neighbours that are not adjacent, or the item
    /// itself.
    #[error("invalid placement: {0}")]
    InvalidPlacement(&'static str),
    #[error(transparent)]
    Position(#[from] super::fractional_index::FractionalIndexError),
    #[error(transparent)]
    Media(#[from] crate::media::MediaError),
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}
