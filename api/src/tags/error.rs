use crate::error::InternalError;
use salvo::oapi::{Components, EndpointOutRegister, Operation, Response as OapiResponse};
use salvo::prelude::*;

#[derive(Debug, thiserror::Error)]
pub enum CreateTagError {
    #[error("tag name already taken")]
    NameTaken,
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for CreateTagError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for CreateTagError {
    fn register(_components: &mut Components, operation: &mut Operation) {
        operation
            .responses
            .insert("409", OapiResponse::new("Conflict"));
        operation
            .responses
            .insert("500", OapiResponse::new("Internal Server Error"));
    }
}

#[async_trait]
impl Writer for CreateTagError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            CreateTagError::NameTaken => {
                res.status_code(StatusCode::CONFLICT);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "NAME_TAKEN", "message": "A tag with that name already exists" }
                })));
            }
            CreateTagError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}

// -----------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum DeleteTagError {
    #[error("not found")]
    NotFound,
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for DeleteTagError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for DeleteTagError {
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
impl Writer for DeleteTagError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            DeleteTagError::NotFound => {
                res.status_code(StatusCode::NOT_FOUND);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": { "code": "NOT_FOUND", "message": "Tag not found" }
                })));
            }
            DeleteTagError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}

// -----------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum SetMediaTagsError {
    /// Unknown tag, or the title is not in the library.
    #[error("not found")]
    NotFound,
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for SetMediaTagsError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for SetMediaTagsError {
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
impl Writer for SetMediaTagsError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            SetMediaTagsError::NotFound => {
                res.status_code(StatusCode::NOT_FOUND);
                res.render(salvo::writing::Json(serde_json::json!({
                    "error": {
                        "code": "NOT_FOUND",
                        "message": "Unknown tag, or the title is not in the library"
                    }
                })));
            }
            SetMediaTagsError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}
