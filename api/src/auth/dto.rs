use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OidcLoginResponse {
    pub auth_url: String,
}

/// Body of `POST /auth/callback`.
///
/// The nonce is deliberately absent: it lives server-side, keyed by `state`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OidcCallbackRequest {
    pub code: String,
    pub state: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OidcCallbackResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RefreshResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateApiTokenRequest {
    pub label: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ApiTokenResponse {
    pub id: String,
    pub label: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<String>,
    pub created_at: String,
}

/// `plaintext` is shown exactly once at creation time.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateApiTokenResponse {
    pub token: ApiTokenResponse,
    pub plaintext: String,
}
