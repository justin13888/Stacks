use crate::error::InternalError;
use salvo::oapi::{Components, EndpointOutRegister, Operation, Response as OapiResponse};
use salvo::prelude::*;

#[derive(Debug, thiserror::Error)]
pub enum DiscoverError {
    #[error("internal error")]
    Internal(InternalError),
}

impl From<InternalError> for DiscoverError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

impl EndpointOutRegister for DiscoverError {
    fn register(_components: &mut Components, operation: &mut Operation) {
        operation
            .responses
            .insert("500", OapiResponse::new("Internal Server Error"));
    }
}

#[async_trait]
impl Writer for DiscoverError {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        match self {
            DiscoverError::Internal(e) => e.write(req, depot, res).await,
        }
    }
}
