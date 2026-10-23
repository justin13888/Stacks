use crate::error::InternalError;
use salvo::oapi::{Components, EndpointOutRegister, Operation, Response as OapiResponse};
use salvo::prelude::*;

#[derive(Debug, thiserror::Error)]
pub enum GetStatusError {
    #[error("not found")]
    NotFound,
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for GetStatusError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for GetStatusError {
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
impl Writer for GetStatusError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            GetStatusError::NotFound => {
                res.status_code(StatusCode::NOT_FOUND);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "NOT_FOUND", "message": "Watch status not found" }
                })));
            }
            GetStatusError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}

// -----------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum SetStatusError {
    #[error("invalid status")]
    InvalidStatus,
    #[error("invalid score")]
    InvalidScore,
    #[error("media not found")]
    MediaNotFound,
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for SetStatusError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for SetStatusError {
    fn register(_components: &mut Components, operation: &mut Operation) {
        operation
            .responses
            .insert("404", OapiResponse::new("Not Found"));
        operation
            .responses
            .insert("422", OapiResponse::new("Unprocessable Entity"));
        operation
            .responses
            .insert("500", OapiResponse::new("Internal Server Error"));
    }
}

#[async_trait]
impl Writer for SetStatusError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            SetStatusError::InvalidStatus => {
                res.status_code(StatusCode::UNPROCESSABLE_ENTITY);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "INVALID_STATUS", "message": "Invalid watch status value" }
                })));
            }
            SetStatusError::InvalidScore => {
                res.status_code(StatusCode::UNPROCESSABLE_ENTITY);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "INVALID_SCORE", "message": "Score is out of range" }
                })));
            }
            SetStatusError::MediaNotFound => {
                res.status_code(StatusCode::NOT_FOUND);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "NOT_FOUND", "message": "Media not found" }
                })));
            }
            SetStatusError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}

// -----------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum DeleteStatusError {
    #[error("not found")]
    NotFound,
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for DeleteStatusError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for DeleteStatusError {
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
impl Writer for DeleteStatusError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            DeleteStatusError::NotFound => {
                res.status_code(StatusCode::NOT_FOUND);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "NOT_FOUND", "message": "Watch status not found" }
                })));
            }
            DeleteStatusError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}

// -----------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum BulkUpdateError {
    #[error("invalid status")]
    InvalidStatus,
    #[error("invalid score")]
    InvalidScore,
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for BulkUpdateError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for BulkUpdateError {
    fn register(_components: &mut Components, operation: &mut Operation) {
        operation
            .responses
            .insert("422", OapiResponse::new("Unprocessable Entity"));
        operation
            .responses
            .insert("500", OapiResponse::new("Internal Server Error"));
    }
}

#[async_trait]
impl Writer for BulkUpdateError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            BulkUpdateError::InvalidStatus => {
                res.status_code(StatusCode::UNPROCESSABLE_ENTITY);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "INVALID_STATUS", "message": "Invalid watch status value" }
                })));
            }
            BulkUpdateError::InvalidScore => {
                res.status_code(StatusCode::UNPROCESSABLE_ENTITY);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "INVALID_SCORE", "message": "Score is out of range" }
                })));
            }
            BulkUpdateError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}
