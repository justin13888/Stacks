use salvo::prelude::*;
use stacks_core::user::service::UserService;

use crate::app::AppState;
use crate::error::IntoInternal;
use crate::extractors::auth_user::require_user;

use super::dto::*;
use super::error::*;

fn user_to_dto(u: stacks_core::entity::user::Model) -> UserResponse {
    UserResponse {
        id: u.id,
        username: u.username,
        email: u.email.unwrap_or_default(),
        role: u.role,
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
    let updated = if let Some(username) = body.username {
        svc.update_username(&user.id, username)
            .await
            .map_err(|e| match e {
                stacks_core::user::UserError::UsernameTaken => UpdateUserError::UsernameTaken,
                stacks_core::user::UserError::NotFound => UpdateUserError::NotFound,
                e => UpdateUserError::Internal(crate::internal!(e)),
            })?
    } else {
        user
    };

    Ok(Json(user_to_dto(updated)))
}

/// Update the authenticated user's application settings.
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
        .update_settings(&user.id, body.settings)
        .await
        .map_err(|e| UpdateUserError::Internal(crate::internal!(e)))?;

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

/// Delete a user by ID (admin only).
#[salvo::oapi::endpoint(tags("User"), summary = "Delete user (admin)")]
pub async fn delete_user(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<()>, DeleteUserError> {
    let id = req.param::<String>("id").unwrap_or_default();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = UserService::new(state.db);
    let deleted = svc
        .delete(&id)
        .await
        .map_err(|e| DeleteUserError::Internal(crate::internal!(e)))?;
    if !deleted {
        return Err(DeleteUserError::NotFound);
    }
    Ok(Json(()))
}
