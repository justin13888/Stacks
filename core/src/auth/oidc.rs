use argon2::password_hash::rand_core::{OsRng, RngCore};
use base64::{Engine as _, engine::general_purpose};
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

#[derive(Debug, Deserialize)]
struct OidcDiscovery {
    authorization_endpoint: String,
    token_endpoint: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    id_token: Option<String>,
    access_token: Option<String>,
}

fn random_hex(len: usize) -> String {
    let mut bytes = vec![0u8; len];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

async fn discover(issuer: &str) -> Result<OidcDiscovery, AuthError> {
    let url = format!(
        "{}/.well-known/openid-configuration",
        issuer.trim_end_matches('/')
    );
    reqwest::get(&url)
        .await
        .map_err(|e| AuthError::OidcError(format!("discovery request failed: {e}")))?
        .json::<OidcDiscovery>()
        .await
        .map_err(|e| AuthError::OidcError(format!("discovery parse failed: {e}")))
}

/// Build the OIDC authorization redirect URL.
pub async fn authorization_url(
    issuer: &str,
    client_id: &str,
    redirect_uri: &str,
) -> Result<AuthorizationParams, AuthError> {
    let discovery = discover(issuer).await?;

    let state = random_hex(32);
    let nonce = random_hex(32);

    let mut url = reqwest::Url::parse(&discovery.authorization_endpoint)
        .map_err(|e| AuthError::OidcError(format!("invalid authorization_endpoint: {e}")))?;

    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", "openid email profile")
        .append_pair("state", &state)
        .append_pair("nonce", &nonce);

    Ok(AuthorizationParams {
        auth_url: url.to_string(),
        state,
        nonce,
    })
}

/// Exchange authorization code for tokens and extract user info.
pub async fn exchange_code(
    issuer: &str,
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
    code: &str,
    _state: &str,
    expected_nonce: &str,
) -> Result<OidcUserInfo, AuthError> {
    let discovery = discover(issuer).await?;

    let client = reqwest::Client::new();
    let token_resp = client
        .post(&discovery.token_endpoint)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", client_id),
            ("client_secret", client_secret),
        ])
        .send()
        .await
        .map_err(|e| AuthError::OidcError(format!("token request failed: {e}")))?
        .json::<TokenResponse>()
        .await
        .map_err(|e| AuthError::OidcError(format!("token response parse failed: {e}")))?;

    let id_token = token_resp
        .id_token
        .or(token_resp.access_token)
        .ok_or_else(|| AuthError::OidcError("no id_token in token response".into()))?;

    // Decode the JWT payload without signature verification.
    let parts: Vec<&str> = id_token.split('.').collect();
    if parts.len() != 3 {
        return Err(AuthError::OidcError("malformed id_token".into()));
    }
    let payload_bytes = general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|e| AuthError::OidcError(format!("id_token base64 decode failed: {e}")))?;
    let claims: serde_json::Value = serde_json::from_slice(&payload_bytes)
        .map_err(|e| AuthError::OidcError(format!("id_token claims parse failed: {e}")))?;

    // Validate nonce claim.
    if let Some(nonce_claim) = claims.get("nonce").and_then(|v| v.as_str())
        && nonce_claim != expected_nonce {
            return Err(AuthError::OidcError("nonce mismatch".into()));
        }

    let sub = claims
        .get("sub")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AuthError::OidcError("missing sub claim".into()))?
        .to_string();

    let email = claims
        .get("email")
        .and_then(|v| v.as_str())
        .map(String::from);

    let preferred_username = claims
        .get("preferred_username")
        .and_then(|v| v.as_str())
        .map(String::from);

    Ok(OidcUserInfo {
        sub,
        email,
        preferred_username,
    })
}
