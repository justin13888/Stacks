use salvo::prelude::*;

use super::error::*;

/// Receive a webhook payload from a Beam media server.
#[salvo::oapi::endpoint(tags("Webhooks"), summary = "Handle Beam webhook")]
pub async fn handle_beam_webhook(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<(), WebhookError> {
    todo!()
}
