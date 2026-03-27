use salvo::prelude::*;

use super::dto::*;
use super::error::*;

/// List all notification channels for the authenticated user.
#[salvo::oapi::endpoint(tags("Notifications"), summary = "List notification channels")]
pub async fn list_channels(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<Vec<ChannelResponse>>, CreateChannelError> {
    todo!()
}

/// Create a new notification channel.
#[salvo::oapi::endpoint(tags("Notifications"), summary = "Create notification channel")]
pub async fn create_channel(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<ChannelResponse>, CreateChannelError> {
    todo!()
}

/// Update an existing notification channel.
#[salvo::oapi::endpoint(tags("Notifications"), summary = "Update notification channel")]
pub async fn update_channel(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<ChannelResponse>, UpdateChannelError> {
    todo!()
}

/// Delete a notification channel by ID.
#[salvo::oapi::endpoint(tags("Notifications"), summary = "Delete notification channel")]
pub async fn delete_channel(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<()>, DeleteChannelError> {
    todo!()
}

/// Get the authenticated user's notification preferences.
#[salvo::oapi::endpoint(tags("Notifications"), summary = "Get notification preferences")]
pub async fn get_preferences(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<PreferencesResponse>, UpdatePrefsError> {
    todo!()
}

/// Replace the authenticated user's notification preferences.
#[salvo::oapi::endpoint(tags("Notifications"), summary = "Update notification preferences")]
pub async fn update_preferences(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<PreferencesResponse>, UpdatePrefsError> {
    todo!()
}
