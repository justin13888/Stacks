use salvo::prelude::*;
use stacks_core::entity::enums::Role;
use stacks_core::user::{UserError, UserService};

use crate::app::AppState;
use crate::error::IntoInternal;
use crate::extractors::auth_user::require_user;
use crate::extractors::uuid_param;

use super::dto::*;
use super::error::*;

fn user_to_dto(u: stacks_core::entity::user::Model) -> UserResponse {
    UserResponse {
        id: u.id.to_string(),
        username: u.display_name,
        email: u.email.unwrap_or_default(),
        role: u.role.to_string(),
        settings: u.settings,
        created_at: u.created_at.to_rfc3339(),
        updated_at: u.updated_at.to_rfc3339(),
    }
}

/// Get the authenticated user's own profile.
#[salvo::oapi::endpoint(tags("User"), summary = "Get current user")]
pub async fn get_me(depot: &mut Depot) -> Result<Json<UserResponse>, GetUserError> {
    // AuthMiddleware guarantees the user is present.
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    Ok(Json(user_to_dto(user)))
}

/// Update the authenticated user's profile fields.
#[salvo::oapi::endpoint(tags("User"), summary = "Update current user")]
pub async fn update_me(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<UserResponse>, UpdateUserError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let body: UpdateUserRequest = req.parse_json().await.into_internal()?;

    let svc = UserService::new(state.db);
    let updated = if let Some(name) = body.username {
        svc.update_display_name(user.id, name)
            .await
            .map_err(|e| match e {
                e @ UserError::EmptyDisplayName => UpdateUserError::InvalidRequest(e.to_string()),
                UserError::NotFound => UpdateUserError::NotFound,
                e => UpdateUserError::Internal(crate::internal!(e)),
            })?
    } else {
        user
    };

    Ok(Json(user_to_dto(updated)))
}

/// Merge into the authenticated user's settings: keys present replace,
/// `null` removes, keys not mentioned are kept.
#[salvo::oapi::endpoint(tags("User"), summary = "Update user settings")]
pub async fn update_settings(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<UserResponse>, UpdateUserError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let body: UpdateSettingsRequest = req.parse_json().await.into_internal()?;

    let svc = UserService::new(state.db);
    let updated = svc
        .update_settings(user.id, body.settings)
        .await
        .map_err(|e| match e {
            e @ UserError::InvalidSettings(_) => UpdateUserError::InvalidRequest(e.to_string()),
            e => UpdateUserError::Internal(crate::internal!(e)),
        })?;

    Ok(Json(user_to_dto(updated)))
}

/// List all users (admin only).
#[salvo::oapi::endpoint(tags("User"), summary = "List users (admin)")]
pub async fn list_users(depot: &mut Depot) -> Result<Json<UserListResponse>, GetUserError> {
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = UserService::new(state.db);
    let users = svc
        .list()
        .await
        .map_err(|e| GetUserError::Internal(crate::internal!(e)))?;
    Ok(Json(UserListResponse {
        users: users.into_iter().map(user_to_dto).collect(),
    }))
}

/// Change a user's role (admin only).
///
/// Complements the `WW_ADMIN_EMAILS` bootstrap: once one admin exists, further
/// role changes no longer need a redeploy or a manual SQL update.
#[salvo::oapi::endpoint(tags("User"), summary = "Set user role (admin)")]
pub async fn update_user_role(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<UserResponse>, UpdateUserError> {
    let id = uuid_param(req, "id").ok_or(UpdateUserError::NotFound)?;
    let state = depot.obtain::<AppState>().unwrap().clone();
    let body: UpdateRoleRequest = req.parse_json().await.into_internal()?;
    let role = Role::parse(&body.role).ok_or(UpdateUserError::InvalidRole)?;

    let svc = UserService::new(state.db);
    let updated = svc.update_role(id, role).await.map_err(|e| match e {
        UserError::NotFound => UpdateUserError::NotFound,
        e => UpdateUserError::Internal(crate::internal!(e)),
    })?;

    Ok(Json(user_to_dto(updated)))
}

/// Deactivate a user (admin only). Their data is kept and the account can be
/// reactivated; permanent removal is a separate, confirmed purge.
#[salvo::oapi::endpoint(tags("User"), summary = "Deactivate user (admin)")]
pub async fn delete_user(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<()>, DeleteUserError> {
    let id = uuid_param(req, "id").ok_or(DeleteUserError::NotFound)?;
    let state = depot.obtain::<AppState>().unwrap().clone();
    UserService::new(state.db)
        .deactivate(id)
        .await
        .map_err(|e| match e {
            UserError::NotFound => DeleteUserError::NotFound,
            e => DeleteUserError::Internal(crate::internal!(e)),
        })?;
    Ok(Json(()))
}
