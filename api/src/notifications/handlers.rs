use salvo::prelude::*;
use stacks_core::notifications::{NotificationError, NotificationService};

use crate::app::AppState;
use crate::error::IntoInternal;
use crate::extractors::auth_user::require_user;

use super::dto::*;
use super::error::*;

fn channel_to_dto(c: stacks_core::entity::notification_channel::Model) -> ChannelResponse {
    ChannelResponse {
        id: c.id,
        channel_type: c.channel_type,
        name: c.name,
        enabled: c.enabled,
        created_at: c.created_at.to_rfc3339(),
    }
}

/// List all notification channels for the authenticated user.
#[salvo::oapi::endpoint(tags("Notifications"), summary = "List notification channels")]
pub async fn list_channels(
    depot: &mut Depot,
) -> Result<Json<Vec<ChannelResponse>>, CreateChannelError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = NotificationService::new(state.db);
    let channels = svc
        .list_channels(&user.id)
        .await
        .map_err(|e| CreateChannelError::Internal(crate::internal!(e)))?;
    Ok(Json(channels.into_iter().map(channel_to_dto).collect()))
}

/// Create a new notification channel.
#[salvo::oapi::endpoint(tags("Notifications"), summary = "Create notification channel")]
pub async fn create_channel(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<ChannelResponse>, CreateChannelError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let body: CreateChannelRequest = req.parse_json().await.into_internal()?;

    let svc = NotificationService::new(state.db);
    let channel = svc
        .create_channel(&user.id, body.channel_type, body.name, body.config)
        .await
        .map_err(|e| CreateChannelError::Internal(crate::internal!(e)))?;
    Ok(Json(channel_to_dto(channel)))
}

/// Update an existing notification channel.
#[salvo::oapi::endpoint(tags("Notifications"), summary = "Update notification channel")]
pub async fn update_channel(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<ChannelResponse>, UpdateChannelError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let id = req.param::<String>("id").unwrap_or_default();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let body: UpdateChannelRequest = req.parse_json().await.into_internal()?;

    let svc = NotificationService::new(state.db);
    let channel = svc
        .update_channel(&user.id, &id, body.name, body.config, body.enabled)
        .await
        .map_err(|e| match e {
            NotificationError::ChannelNotFound => UpdateChannelError::NotFound,
            e => UpdateChannelError::Internal(crate::internal!(e)),
        })?;
    Ok(Json(channel_to_dto(channel)))
}

/// Delete a notification channel by ID.
#[salvo::oapi::endpoint(tags("Notifications"), summary = "Delete notification channel")]
pub async fn delete_channel(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<()>, DeleteChannelError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let id = req.param::<String>("id").unwrap_or_default();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = NotificationService::new(state.db);
    svc.delete_channel(&user.id, &id)
        .await
        .map_err(|e| match e {
            NotificationError::ChannelNotFound => DeleteChannelError::NotFound,
            e => DeleteChannelError::Internal(crate::internal!(e)),
        })?;
    Ok(Json(()))
}

/// Event types a notification preference may target. Mirrors
/// `stacks_core::notifications::NotificationEvent::event_type`.
const KNOWN_EVENT_TYPES: [&str; 4] = [
    "new_episode",
    "available_on_beam",
    "upcoming_release",
    "import_completed",
];

/// Get the authenticated user's notification preferences.
#[salvo::oapi::endpoint(tags("Notifications"), summary = "Get notification preferences")]
pub async fn get_preferences(
    depot: &mut Depot,
) -> Result<Json<PreferencesResponse>, UpdatePrefsError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = NotificationService::new(state.db);
    let prefs = svc
        .get_preferences(&user.id)
        .await
        .map_err(|e| UpdatePrefsError::Internal(crate::internal!(e)))?;
    Ok(Json(PreferencesResponse {
        preferences: prefs
            .into_iter()
            .map(NotificationPreference::from)
            .collect(),
    }))
}

/// Replace the authenticated user's notification preferences.
#[salvo::oapi::endpoint(tags("Notifications"), summary = "Update notification preferences")]
pub async fn update_preferences(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<PreferencesResponse>, UpdatePrefsError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let body: UpdatePreferencesRequest = req.parse_json().await.into_internal()?;

    // Reject unknown event types before persisting anything.
    for pref in &body.preferences {
        if !KNOWN_EVENT_TYPES.contains(&pref.event_type.as_str()) {
            return Err(UpdatePrefsError::InvalidEventType(pref.event_type.clone()));
        }
    }

    let svc = NotificationService::new(state.db);
    for pref in body.preferences {
        svc.set_preference(
            &user.id,
            &pref.event_type,
            &pref.channel_id,
            pref.enabled,
            pref.config,
        )
        .await
        .map_err(|e| UpdatePrefsError::Internal(crate::internal!(e)))?;
    }

    let prefs = svc
        .get_preferences(&user.id)
        .await
        .map_err(|e| UpdatePrefsError::Internal(crate::internal!(e)))?;
    Ok(Json(PreferencesResponse {
        preferences: prefs
            .into_iter()
            .map(NotificationPreference::from)
            .collect(),
    }))
}
