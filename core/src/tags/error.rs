use thiserror::Error;

#[derive(Debug, Error)]
pub enum TagError {
    #[error("tag not found")]
    NotFound,
    #[error("a tag with that name already exists")]
    NameTaken,
    #[error("tag name must not be empty")]
    EmptyName,
    #[error("a tag colour is `#RRGGBB`")]
    InvalidColor,
    #[error("the title is not in the library")]
    NotTracked,
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}
