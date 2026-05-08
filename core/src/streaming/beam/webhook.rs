use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::streaming::StreamingError;
use crate::streaming::models::ConnectorEvent;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeamWebhookPayload {
    pub event: String,
    pub item_id: Option<String>,
    pub user: Option<String>,
    pub position_ticks: Option<i64>,
    pub metadata: Option<serde_json::Value>,
}

/// Verify the HMAC-SHA256 signature of an incoming Beam webhook request.
///
/// `signature_header` is expected to be a hex-encoded SHA-256 HMAC of the
/// raw payload using the configured shared secret. A constant-time
/// comparison is used to avoid timing attacks.
pub fn verify_signature(
    payload: &[u8],
    signature_header: Option<&str>,
    secret: &str,
) -> Result<(), StreamingError> {
    let provided = signature_header.ok_or(StreamingError::InvalidWebhookSignature)?;
    let provided_bytes =
        hex::decode(provided).map_err(|_| StreamingError::InvalidWebhookSignature)?;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| StreamingError::InvalidWebhookSignature)?;
    mac.update(payload);
    mac.verify_slice(&provided_bytes)
        .map_err(|_| StreamingError::InvalidWebhookSignature)
}

/// Parse raw webhook bytes into a typed `BeamWebhookPayload`.
pub fn parse_payload(payload: &[u8]) -> Result<BeamWebhookPayload, StreamingError> {
    serde_json::from_slice(payload)
        .map_err(|e| StreamingError::Upstream(format!("invalid beam payload: {e}")))
}

/// Convert a `BeamWebhookPayload` into a list of `ConnectorEvent`s.
pub fn to_connector_events(
    payload: BeamWebhookPayload,
) -> Result<Vec<ConnectorEvent>, StreamingError> {
    let position = payload.position_ticks.map(|t| (t / 10_000_000) as i32);
    let server_internal_id = payload.item_id.clone().unwrap_or_default();
    let user = payload.user.clone();

    let event = match payload.event.as_str() {
        "playback.started" | "play" => ConnectorEvent::PlaybackStarted {
            server_internal_id,
            user_id: user,
            position_seconds: position,
        },
        "playback.stopped" | "stop" => ConnectorEvent::PlaybackStopped {
            server_internal_id,
            user_id: user,
            position_seconds: position,
        },
        "playback.paused" | "pause" => ConnectorEvent::PlaybackPaused {
            server_internal_id,
            user_id: user,
            position_seconds: position,
        },
        "playback.finished" | "complete" => ConnectorEvent::PlaybackFinished {
            server_internal_id,
            user_id: user,
        },
        "library.changed" => ConnectorEvent::LibraryChanged {
            server_internal_id: payload.item_id,
        },
        _ => ConnectorEvent::Unknown {
            raw: serde_json::to_value(&payload).unwrap_or(serde_json::Value::Null),
        },
    };
    Ok(vec![event])
}
