use crate::error::InternalError;
use salvo::oapi::{Components, EndpointOutRegister, Operation, Response as OapiResponse};
use salvo::prelude::*;

#[derive(Debug, thiserror::Error)]
pub enum StartImportError {
    #[error("unsupported format")]
    UnsupportedFormat,
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for StartImportError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for StartImportError {
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
impl Writer for StartImportError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            StartImportError::UnsupportedFormat => {
                res.status_code(StatusCode::UNPROCESSABLE_ENTITY);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "UNSUPPORTED_FORMAT", "message": "Import format is not supported" }
                })));
            }
            StartImportError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}

// -----------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum GetImportStatusError {
    #[error("not found")]
    NotFound,
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for GetImportStatusError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for GetImportStatusError {
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
impl Writer for GetImportStatusError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            GetImportStatusError::NotFound => {
                res.status_code(StatusCode::NOT_FOUND);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "NOT_FOUND", "message": "Import job not found" }
                })));
            }
            GetImportStatusError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}

// -----------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("unsupported format")]
    UnsupportedFormat,
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for ExportError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for ExportError {
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
impl Writer for ExportError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            ExportError::UnsupportedFormat => {
                res.status_code(StatusCode::UNPROCESSABLE_ENTITY);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "UNSUPPORTED_FORMAT", "message": "Export format is not supported" }
                })));
            }
            ExportError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}
