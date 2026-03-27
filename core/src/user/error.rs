use thiserror::Error;

#[derive(Debug, Error)]
pub enum UserError {
    #[error("user not found")]
    NotFound,
    #[error("username already taken")]
    UsernameTaken,
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}
