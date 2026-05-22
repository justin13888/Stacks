use std::collections::HashMap;

use salvo::prelude::*;
use stacks_core::crypto;
use stacks_core::streaming::{StreamingError, StreamingService};

use crate::app::AppState;

use super::error::*;

/// Receive a webhook payload from a Beam media server.
///
/// This route is intentionally public — the HMAC signature carried in the
/// request headers, verified against the server's stored webhook secret, is
/// the authentication mechanism. A `2xx` response acknowledges the delivery.
#[salvo::oapi::endpoint(tags("Webhooks"), summary = "Handle Beam webhook")]
pub async fn handle_beam_webhook(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<serde_json::Value>, WebhookError> {
    let state = depot.obtain::<AppState>().unwrap().clone();
    let server_id = req.param::<String>("server_id").unwrap_or_default();

    // Snapshot the headers (lowercased) before the body borrow.
    let headers: HashMap<String, String> = req
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.as_str().to_ascii_lowercase(), v.to_string()))
        })
        .collect();

    // The signature is computed over the raw body, so read the bytes verbatim.
    let payload = req
        .payload()
        .await
        .map_err(|e| WebhookError::Internal(crate::internal!(e)))?
        .clone();

    let svc = StreamingService::new(
        state.db.clone(),
        crypto::derive_key(&state.config.auth.secret_key),
    );
    svc.process_webhook(&server_id, payload.as_ref(), &headers)
        .await
        .map_err(|e| match e {
            StreamingError::ServerNotFound => WebhookError::ServerNotFound,
            StreamingError::InvalidWebhookSignature => WebhookError::InvalidSignature,
            e => WebhookError::Internal(crate::internal!(e)),
        })?;
    Ok(Json(serde_json::json!({ "status": "ok" })))
}
