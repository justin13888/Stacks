use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::streaming::StreamingError;
use crate::streaming::models::ConnectorEvent;

type HmacSha256 = Hmac<Sha256>;

/// Number of 100-nanosecond ticks in one second (Jellyfin/Emby convention).
const TICKS_PER_SECOND: i64 = 10_000_000;

/// Webhook payload as POSTed by a Beam server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeamWebhookPayload {
    pub event: String,
    pub item_id: Option<String>,
    pub user: Option<String>,
    /// Playback position, in 100-nanosecond ticks.
    pub position_ticks: Option<i64>,
    pub metadata: Option<serde_json::Value>,
}

/// Verify the HMAC-SHA256 signature of an incoming Beam webhook request.
///
/// The signature header carries a hex-encoded HMAC of the raw request body,
/// optionally prefixed with `sha256=`. The comparison is constant-time.
pub fn verify_signature(
    payload: &[u8],
    signature_header: Option<&str>,
    secret: &str,
) -> Result<(), StreamingError> {
    let provided = signature_header
        .ok_or(StreamingError::InvalidWebhookSignature)?
        .trim();
    let provided = provided.strip_prefix("sha256=").unwrap_or(provided);
    let provided = hex::decode(provided).map_err(|_| StreamingError::InvalidWebhookSignature)?;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| StreamingError::InvalidWebhookSignature)?;
    mac.update(payload);
    mac.verify_slice(&provided)
        .map_err(|_| StreamingError::InvalidWebhookSignature)
}

/// Parse raw webhook bytes into a typed [`BeamWebhookPayload`].
pub fn parse_payload(payload: &[u8]) -> Result<BeamWebhookPayload, StreamingError> {
    serde_json::from_slice(payload).map_err(|e| StreamingError::Payload(e.to_string()))
}

/// Convert a [`BeamWebhookPayload`] into connector events.
///
/// Beam's event names vary by version, so the match normalises punctuation and
/// accepts the common spellings; unrecognised events become
/// [`ConnectorEvent::Unknown`] rather than being dropped.
pub fn to_connector_events(
    payload: BeamWebhookPayload,
) -> Result<Vec<ConnectorEvent>, StreamingError> {
    let position_seconds = payload
        .position_ticks
        .map(|ticks| (ticks / TICKS_PER_SECOND) as i32);
    let server_internal_id = payload.item_id.clone().unwrap_or_default();
    let user_id = payload.user.clone();

    let normalised = payload
        .event
        .to_ascii_lowercase()
        .replace(['.', '-', ' '], "_");
    let event = match normalised.as_str() {
        "playback_start" | "playback_started" | "play" => ConnectorEvent::PlaybackStarted {
            server_internal_id,
            user_id,
            position_seconds,
        },
        "playback_stop" | "playback_stopped" | "stop" => ConnectorEvent::PlaybackStopped {
            server_internal_id,
            user_id,
            position_seconds,
        },
        "playback_pause" | "playback_paused" | "pause" => ConnectorEvent::PlaybackPaused {
            server_internal_id,
            user_id,
            position_seconds,
        },
        "playback_finish" | "playback_finished" | "playback_complete" | "finished" => {
            ConnectorEvent::PlaybackFinished {
                server_internal_id,
                user_id,
            }
        }
        "library_changed" | "library_update" | "library_updated" | "scan_complete" => {
            ConnectorEvent::LibraryChanged {
                server_internal_id: payload.item_id.clone(),
            }
        }
        _ => ConnectorEvent::Unknown {
            raw: serde_json::to_value(&payload).unwrap_or(serde_json::Value::Null),
        },
    };
    Ok(vec![event])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_round_trips() {
        let body = br#"{"event":"play"}"#;
        let secret = "topsecret";
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let sig = hex::encode(mac.finalize().into_bytes());

        assert!(verify_signature(body, Some(&sig), secret).is_ok());
        assert!(verify_signature(body, Some(&format!("sha256={sig}")), secret).is_ok());
        assert!(verify_signature(body, Some(&sig), "wrong").is_err());
        assert!(verify_signature(body, None, secret).is_err());
    }

    #[test]
    fn maps_known_events() {
        let payload = BeamWebhookPayload {
            event: "Playback.Finished".to_string(),
            item_id: Some("abc".to_string()),
            user: Some("u1".to_string()),
            position_ticks: Some(30 * TICKS_PER_SECOND),
            metadata: None,
        };
        let events = to_connector_events(payload).unwrap();
        assert!(matches!(
            events.as_slice(),
            [ConnectorEvent::PlaybackFinished { .. }]
        ));
    }
}
