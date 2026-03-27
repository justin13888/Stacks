use serde::{Deserialize, Serialize};

use super::AuthError;

#[derive(Debug, Serialize, Deserialize)]
pub struct OidcUserInfo {
    pub sub: String,
    pub email: Option<String>,
    pub preferred_username: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthorizationParams {
    pub auth_url: String,
    pub state: String,
    pub nonce: String,
}

/// Build the OIDC authorization redirect URL.
pub async fn authorization_url(
    issuer: &str,
    client_id: &str,
    redirect_uri: &str,
) -> Result<AuthorizationParams, AuthError> {
    // TODO: implement OIDC discovery + PKCE flow
    let _ = (issuer, client_id, redirect_uri);
    todo!("OIDC authorization URL construction")
}

/// Exchange authorization code for tokens and extract user info.
pub async fn exchange_code(
    issuer: &str,
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
    code: &str,
    state: &str,
    expected_nonce: &str,
) -> Result<OidcUserInfo, AuthError> {
    let _ = (
        issuer,
        client_id,
        client_secret,
        redirect_uri,
        code,
        state,
        expected_nonce,
    );
    todo!("OIDC code exchange")
}
