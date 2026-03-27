//! InternalError — wraps unexpected errors (DB failures, serialization, etc.).
//! Logs the real cause and returns a consistent HTTP 500 with no internal detail.

use salvo::prelude::*;
use std::fmt;
use tracing::error;

/// Wraps an internal error. Logs on construction, returns opaque 500 to the client.
#[derive(Debug)]
pub struct InternalError {
    #[allow(dead_code)]
    message: String,
}

impl InternalError {
    pub fn new(message: impl Into<String>) -> Self {
        let message = message.into();
        error!(error = %message, "internal server error");
        Self { message }
    }
}

impl fmt::Display for InternalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "internal server error")
    }
}

impl std::error::Error for InternalError {}

/// Convert any Display error into InternalError using the `?` operator via a helper trait.
pub trait IntoInternal<T> {
    fn into_internal(self) -> Result<T, InternalError>;
}

impl<T, E: fmt::Display> IntoInternal<T> for Result<T, E> {
    fn into_internal(self) -> Result<T, InternalError> {
        self.map_err(|e| InternalError::new(e.to_string()))
    }
}

/// Salvo Writer impl: always emits HTTP 500 with a fixed JSON body.
#[async_trait]
impl Writer for InternalError {
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(salvo::writing::Json(serde_json::json!({
            "error": {
                "code": "INTERNAL_ERROR",
                "message": "An internal error occurred"
            }
        })));
    }
}

/// Convenience macro: map any error to InternalError.
#[macro_export]
macro_rules! internal {
    ($e:expr) => {
        $crate::error::InternalError::new(format!("{}", $e))
    };
}
