use crate::error::InternalError;
use salvo::oapi::{Components, EndpointOutRegister, Operation, Response as OapiResponse};
use salvo::prelude::*;

#[derive(Debug, thiserror::Error)]
#[error("oidc login error: {0}")]
pub struct OidcLoginError(pub InternalError);

impl From<InternalError> for OidcLoginError {
    fn from(e: InternalError) -> Self {
        Self(e)
    }
}

impl EndpointOutRegister for OidcLoginError {
    fn register(_components: &mut Components, operation: &mut Operation) {
        operation
            .responses
            .insert("500", OapiResponse::new("Internal Server Error"));
    }
}

#[async_trait]
impl Writer for OidcLoginError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        self.0.write(req, depot, res).await;
    }
}

// -----------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum OidcCallbackError {
    #[error("invalid state")]
    InvalidState,
    #[error("invalid code")]
    InvalidCode,
    #[error("provisioning failed")]
    ProvisioningFailed,
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for OidcCallbackError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for OidcCallbackError {
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
impl Writer for OidcCallbackError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            OidcCallbackError::InvalidState => {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "INVALID_STATE", "message": "Invalid OAuth state parameter" }
                })));
            }
            OidcCallbackError::InvalidCode => {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "INVALID_CODE", "message": "Invalid authorization code" }
                })));
            }
            OidcCallbackError::ProvisioningFailed => {
                res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "PROVISIONING_FAILED", "message": "User provisioning failed" }
                })));
            }
            OidcCallbackError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}

// -----------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum RefreshError {
    #[error("invalid token")]
    InvalidToken,
    #[error("token expired")]
    TokenExpired,
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for RefreshError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for RefreshError {
    fn register(_components: &mut Components, operation: &mut Operation) {
        operation
            .responses
            .insert("401", OapiResponse::new("Unauthorized"));
        operation
            .responses
            .insert("500", OapiResponse::new("Internal Server Error"));
    }
}

#[async_trait]
impl Writer for RefreshError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            RefreshError::InvalidToken => {
                res.status_code(StatusCode::UNAUTHORIZED);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "INVALID_TOKEN", "message": "Refresh token is invalid" }
                })));
            }
            RefreshError::TokenExpired => {
                res.status_code(StatusCode::UNAUTHORIZED);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "TOKEN_EXPIRED", "message": "Refresh token has expired" }
                })));
            }
            RefreshError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}

// -----------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum CreateTokenError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for CreateTokenError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for CreateTokenError {
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
impl Writer for CreateTokenError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            CreateTokenError::InvalidRequest(msg) => {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "INVALID_REQUEST", "message": msg }
                })));
            }
            CreateTokenError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}

// -----------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum DeleteTokenError {
    #[error("not found")]
    NotFound,
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for DeleteTokenError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for DeleteTokenError {
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
impl Writer for DeleteTokenError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            DeleteTokenError::NotFound => {
                res.status_code(StatusCode::NOT_FOUND);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "NOT_FOUND", "message": "Token not found" }
                })));
            }
            DeleteTokenError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}
