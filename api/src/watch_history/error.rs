use crate::error::InternalError;
use salvo::oapi::{Components, EndpointOutRegister, Operation, Response as OapiResponse};
use salvo::prelude::*;

#[derive(Debug, thiserror::Error)]
pub enum ListHistoryError {
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for ListHistoryError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for ListHistoryError {
    fn register(_components: &mut Components, operation: &mut Operation) {
        operation
            .responses
            .insert("500", OapiResponse::new("Internal Server Error"));
    }
}

#[async_trait]
impl Writer for ListHistoryError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            ListHistoryError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}

// -----------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum AddHistoryError {
    #[error("invalid event type")]
    InvalidEvent,
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for AddHistoryError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for AddHistoryError {
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
impl Writer for AddHistoryError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            AddHistoryError::InvalidEvent => {
                res.status_code(StatusCode::UNPROCESSABLE_ENTITY);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "INVALID_EVENT", "message": "Invalid history event type" }
                })));
            }
            AddHistoryError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}
