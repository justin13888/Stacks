use crate::error::InternalError;
use salvo::oapi::{Components, EndpointOutRegister, Operation, Response as OapiResponse};
use salvo::prelude::*;

#[derive(Debug, thiserror::Error)]
pub enum GetUserError {
    #[error("not found")]
    NotFound,
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for GetUserError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for GetUserError {
    fn register(_components: &mut Components, operation: &mut Operation) {
        operation
            .responses
            .insert("404", OapiResponse::new("Not Found"));
        operation
            .responses
            .insert("500", OapiResponse::new("Internal Server Error"));
    }
}

#[async_trait]
impl Writer for GetUserError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            GetUserError::NotFound => {
                res.status_code(StatusCode::NOT_FOUND);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "NOT_FOUND", "message": "User not found" }
                })));
            }
            GetUserError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}

// -----------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum UpdateUserError {
    #[error("not found")]
    NotFound,
    #[error("username taken")]
    UsernameTaken,
    #[error("invalid role")]
    InvalidRole,
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for UpdateUserError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for UpdateUserError {
    fn register(_components: &mut Components, operation: &mut Operation) {
        operation
            .responses
            .insert("404", OapiResponse::new("Not Found"));
        operation
            .responses
            .insert("400", OapiResponse::new("Bad Request"));
        operation
            .responses
            .insert("409", OapiResponse::new("Conflict"));
        operation
            .responses
            .insert("500", OapiResponse::new("Internal Server Error"));
    }
}

#[async_trait]
impl Writer for UpdateUserError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            UpdateUserError::NotFound => {
                res.status_code(StatusCode::NOT_FOUND);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "NOT_FOUND", "message": "User not found" }
                })));
            }
            UpdateUserError::UsernameTaken => {
                res.status_code(StatusCode::CONFLICT);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "USERNAME_TAKEN", "message": "Username is already taken" }
                })));
            }
            UpdateUserError::InvalidRole => {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "INVALID_ROLE", "message": "Role must be 'user' or 'admin'" }
                })));
            }
            UpdateUserError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}

// -----------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum DeleteUserError {
    #[error("not found")]
    NotFound,
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for DeleteUserError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for DeleteUserError {
    fn register(_components: &mut Components, operation: &mut Operation) {
        operation
            .responses
            .insert("404", OapiResponse::new("Not Found"));
        operation
            .responses
            .insert("500", OapiResponse::new("Internal Server Error"));
    }
}

#[async_trait]
impl Writer for DeleteUserError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            DeleteUserError::NotFound => {
                res.status_code(StatusCode::NOT_FOUND);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "NOT_FOUND", "message": "User not found" }
                })));
            }
            DeleteUserError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}
