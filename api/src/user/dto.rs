use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UserResponse {
    pub id: String,
    pub username: String,
    pub email: String,
    pub role: String,
    pub settings: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateUserRequest {
    pub username: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateSettingsRequest {
    pub settings: serde_json::Value,
}

/// Body of `PATCH /admin/users/{id}/role`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateRoleRequest {
    /// Either `user` or `admin`.
    pub role: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UserListResponse {
    pub users: Vec<UserResponse>,
}
