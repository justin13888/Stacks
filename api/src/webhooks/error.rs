use crate::error::InternalError;
use salvo::oapi::{Components, EndpointOutRegister, Operation, Response as OapiResponse};
use salvo::prelude::*;

#[derive(Debug, thiserror::Error)]
pub enum WebhookError {
    #[error("invalid signature")]
    InvalidSignature,
    #[error("server not found")]
    ServerNotFound,
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for WebhookError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for WebhookError {
    fn register(_components: &mut Components, operation: &mut Operation) {
        operation
            .responses
            .insert("401", OapiResponse::new("Unauthorized"));
        operation
            .responses
            .insert("404", OapiResponse::new("Not Found"));
        operation
            .responses
            .insert("500", OapiResponse::new("Internal Server Error"));
    }
}

#[async_trait]
impl Writer for WebhookError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            WebhookError::InvalidSignature => {
                res.status_code(StatusCode::UNAUTHORIZED);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "INVALID_SIGNATURE", "message": "Webhook signature verification failed" }
                })));
            }
            WebhookError::ServerNotFound => {
                res.status_code(StatusCode::NOT_FOUND);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "SERVER_NOT_FOUND", "message": "Webhook source server not found" }
                })));
            }
            WebhookError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}
