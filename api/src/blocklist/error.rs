use crate::error::InternalError;
use salvo::oapi::{Components, EndpointOutRegister, Operation, Response as OapiResponse};
use salvo::prelude::*;

#[derive(Debug, thiserror::Error)]
pub enum AddBlockError {
    #[error("media not found")]
    MediaNotFound,
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for AddBlockError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for AddBlockError {
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
impl Writer for AddBlockError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            AddBlockError::MediaNotFound => {
                res.status_code(StatusCode::NOT_FOUND);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "NOT_FOUND", "message": "Media not found" }
                })));
            }
            AddBlockError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}

// -----------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum RemoveBlockError {
    #[error("not found")]
    NotFound,
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for RemoveBlockError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for RemoveBlockError {
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
impl Writer for RemoveBlockError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            RemoveBlockError::NotFound => {
                res.status_code(StatusCode::NOT_FOUND);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "NOT_FOUND", "message": "Blocklist entry not found" }
                })));
            }
            RemoveBlockError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}
