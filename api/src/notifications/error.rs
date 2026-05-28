use crate::error::InternalError;
use salvo::oapi::{Components, EndpointOutRegister, Operation, Response as OapiResponse};
use salvo::prelude::*;

#[derive(Debug, thiserror::Error)]
pub enum CreateChannelError {
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for CreateChannelError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for CreateChannelError {
    fn register(_components: &mut Components, operation: &mut Operation) {
        operation
            .responses
            .insert("500", OapiResponse::new("Internal Server Error"));
    }
}

#[async_trait]
impl Writer for CreateChannelError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            CreateChannelError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}

// -----------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum UpdateChannelError {
    #[error("not found")]
    NotFound,
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for UpdateChannelError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for UpdateChannelError {
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
impl Writer for UpdateChannelError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            UpdateChannelError::NotFound => {
                res.status_code(StatusCode::NOT_FOUND);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "NOT_FOUND", "message": "Notification channel not found" }
                })));
            }
            UpdateChannelError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}

// -----------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum DeleteChannelError {
    #[error("not found")]
    NotFound,
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for DeleteChannelError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for DeleteChannelError {
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
impl Writer for DeleteChannelError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            DeleteChannelError::NotFound => {
                res.status_code(StatusCode::NOT_FOUND);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "NOT_FOUND", "message": "Notification channel not found" }
                })));
            }
            DeleteChannelError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}

// -----------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum UpdatePrefsError {
    #[error("unknown notification event type: {0}")]
    InvalidEventType(String),
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for UpdatePrefsError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for UpdatePrefsError {
    fn register(_components: &mut Components, operation: &mut Operation) {
        operation
            .responses
            .insert("400", OapiResponse::new("Bad Request"));
        operation
            .responses
            .insert("500", OapiResponse::new("Internal Server Error"));
    }
}

#[async_trait]
impl Writer for UpdatePrefsError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            UpdatePrefsError::InvalidEventType(t) => {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": {
                        "code": "INVALID_EVENT_TYPE",
                        "message": format!("Unknown notification event type: {t}"),
                    }
                })));
            }
            UpdatePrefsError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}
