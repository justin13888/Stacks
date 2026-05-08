use std::collections::HashMap;

use salvo::prelude::*;
use stacks_core::streaming::StreamingService;

use crate::app::AppState;

use super::error::*;

/// Receive a webhook payload from a Beam media server.
#[salvo::oapi::endpoint(tags("Webhooks"), summary = "Handle Beam webhook")]
pub async fn handle_beam_webhook(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<()>, WebhookError> {
    let state = depot.obtain::<AppState>().unwrap().clone();
    let server_id = req.param::<String>("server_id").unwrap_or_default();

    let svc = StreamingService::new(state.db.clone(), &state.config.auth.secret_key);
    let server = svc
        .get_server(&server_id)
        .await
        .map_err(|_| WebhookError::ServerNotFound)?;

    let connector = svc
        .connector_for(&server)
        .map_err(|e| WebhookError::Internal(crate::internal!(e)))?;

    // Snapshot headers for the connector — Beam uses HMAC over the raw body.
    let mut headers: HashMap<String, String> = HashMap::new();
    for (k, v) in req.headers().iter() {
        if let Ok(s) = v.to_str() {
            headers.insert(k.as_str().to_string(), s.to_string());
        }
    }

    let body_bytes = req
        .payload()
        .await
        .map_err(|e| WebhookError::Internal(crate::internal!(e)))?
        .to_vec();

    let events = connector
        .handle_webhook(&body_bytes, &headers)
        .await
        .map_err(|e| match e {
            stacks_core::streaming::StreamingError::InvalidWebhookSignature => {
                WebhookError::InvalidSignature
            }
            e => WebhookError::Internal(crate::internal!(e)),
        })?;

    svc.handle_webhook_events(&server_id, events)
        .await
        .map_err(|e| WebhookError::Internal(crate::internal!(e)))?;

    Ok(Json(()))
}
