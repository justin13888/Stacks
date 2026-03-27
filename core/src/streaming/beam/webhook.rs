use serde::{Deserialize, Serialize};

use crate::streaming::StreamingError;
use crate::streaming::models::ConnectorEvent;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeamWebhookPayload {
    pub event: String,
    pub item_id: Option<String>,
    pub user: Option<String>,
    pub position_ticks: Option<i64>,
    pub metadata: Option<serde_json::Value>,
}

/// Verify the HMAC signature of an incoming Beam webhook request.
pub fn verify_signature(
    payload: &[u8],
    signature_header: Option<&str>,
    secret: &str,
) -> Result<(), StreamingError> {
    let _ = (payload, signature_header, secret);
    todo!()
}

/// Parse raw webhook bytes into a typed `BeamWebhookPayload`.
pub fn parse_payload(payload: &[u8]) -> Result<BeamWebhookPayload, StreamingError> {
    let _ = payload;
    todo!()
}

/// Convert a `BeamWebhookPayload` into a list of `ConnectorEvent`s.
pub fn to_connector_events(
    payload: BeamWebhookPayload,
) -> Result<Vec<ConnectorEvent>, StreamingError> {
    let _ = payload;
    todo!()
}
