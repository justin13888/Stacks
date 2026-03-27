use salvo::prelude::*;

use super::dto::*;
use super::error::*;

/// Get the authenticated user's own profile.
#[salvo::oapi::endpoint(tags("User"), summary = "Get current user")]
pub async fn get_me(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<UserResponse>, GetUserError> {
    todo!()
}

/// Update the authenticated user's profile fields.
#[salvo::oapi::endpoint(tags("User"), summary = "Update current user")]
pub async fn update_me(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<UserResponse>, UpdateUserError> {
    todo!()
}

/// Update the authenticated user's application settings.
#[salvo::oapi::endpoint(tags("User"), summary = "Update user settings")]
pub async fn update_settings(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<UserResponse>, UpdateUserError> {
    todo!()
}

/// List all users (admin only).
#[salvo::oapi::endpoint(tags("User"), summary = "List users (admin)")]
pub async fn list_users(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<UserListResponse>, GetUserError> {
    todo!()
}

/// Delete a user by ID (admin only).
#[salvo::oapi::endpoint(tags("User"), summary = "Delete user (admin)")]
pub async fn delete_user(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<()>, DeleteUserError> {
    todo!()
}
