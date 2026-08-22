use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

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

#[derive(Debug, Clone, Deserialize)]
struct OidcDiscovery {
    authorization_endpoint: String,
    token_endpoint: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    id_token: Option<String>,
}

fn random_hex(len: usize) -> String {
    let mut bytes = vec![0u8; len];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Discovery documents are effectively static; re-fetching one on every login
/// *and* every callback added two avoidable round-trips to each sign-in.
const DISCOVERY_TTL: Duration = Duration::from_secs(300);

fn discovery_cache() -> &'static Mutex<HashMap<String, (Instant, OidcDiscovery)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (Instant, OidcDiscovery)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn discover(base: &str) -> Result<OidcDiscovery, AuthError> {
    let base = base.trim_end_matches('/').to_string();

    if let Ok(cache) = discovery_cache().lock()
        && let Some((fetched_at, doc)) = cache.get(&base)
        && fetched_at.elapsed() < DISCOVERY_TTL
    {
        return Ok(doc.clone());
    }

    let url = format!("{base}/.well-known/openid-configuration");
    let doc = reqwest::get(&url)
        .await
        .map_err(|e| AuthError::OidcError(format!("discovery request failed: {e}")))?
        .json::<OidcDiscovery>()
        .await
        .map_err(|e| AuthError::OidcError(format!("discovery parse failed: {e}")))?;

    if let Ok(mut cache) = discovery_cache().lock() {
        cache.insert(base, (Instant::now(), doc.clone()));
    }
    Ok(doc)
}

/// Re-point an endpoint discovered via the internal base URL at the public
/// issuer, so the browser gets a URL it can actually reach.
///
/// Only the origin (and any issuer path prefix) is swapped; the endpoint's own
/// path suffix and query are preserved.
fn rebase_endpoint(endpoint: &str, internal_base: &str, public_base: &str) -> String {
    let internal = internal_base.trim_end_matches('/');
    let public = public_base.trim_end_matches('/');
    match endpoint.strip_prefix(internal) {
        Some(suffix) => format!("{public}{suffix}"),
        // The provider advertised an endpoint outside its own issuer base;
        // trust it as-is rather than mangling it.
        None => endpoint.to_string(),
    }
}

/// Build the OIDC authorization redirect URL.
///
/// `server_base` is used for the discovery call; `public_issuer` is the URL the
/// end user's browser can reach. They differ only in split-horizon deployments
/// (containers, reverse proxies) — see [`OidcConfig::internal_issuer`].
///
/// [`OidcConfig::internal_issuer`]: crate::config::models::OidcConfig::internal_issuer
pub async fn authorization_url(
    server_base: &str,
    public_issuer: &str,
    client_id: &str,
    redirect_uri: &str,
) -> Result<AuthorizationParams, AuthError> {
    let discovery = discover(server_base).await?;

    let state = random_hex(32);
    let nonce = random_hex(32);

    let endpoint = rebase_endpoint(
        &discovery.authorization_endpoint,
        server_base,
        public_issuer,
    );

    let mut url = reqwest::Url::parse(&endpoint)
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

/// Exchange an authorization code for tokens and extract user info.
///
/// `expected_nonce` comes from the server-side `oidc_auth_sessions` row keyed by
/// the `state` the provider echoed back — the client never sees it, so it cannot
/// be forged. `state` itself is validated by the caller before we get here.
pub async fn exchange_code(
    server_base: &str,
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
    code: &str,
    expected_nonce: &str,
) -> Result<OidcUserInfo, AuthError> {
    let discovery = discover(server_base).await?;

    let client = reqwest::Client::new();
    let response = client
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
        .map_err(|e| AuthError::OidcError(format!("token request failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AuthError::OidcError(format!(
            "token endpoint returned {status}: {body}"
        )));
    }

    let token_resp = response
        .json::<TokenResponse>()
        .await
        .map_err(|e| AuthError::OidcError(format!("token response parse failed: {e}")))?;

    // Previously this fell back to `access_token` when `id_token` was absent.
    // An access token is opaque to us and carries no `sub`/`nonce` claims, so
    // that fallback could only ever produce a confusing downstream error.
    let id_token = token_resp
        .id_token
        .ok_or_else(|| AuthError::OidcError("no id_token in token response".into()))?;

    let claims = decode_id_token_claims(&id_token)?;

    // The nonce binds this id_token to the authorization request we started.
    // A provider that omits the claim entirely cannot be checked; one that
    // returns a different value is rejected.
    match claims.get("nonce").and_then(|v| v.as_str()) {
        Some(n) if n != expected_nonce => {
            return Err(AuthError::OidcError("nonce mismatch".into()));
        }
        _ => {}
    }

    let sub = claims
        .get("sub")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AuthError::OidcError("missing sub claim".into()))?
        .to_string();

    Ok(OidcUserInfo {
        sub,
        email: claims
            .get("email")
            .and_then(|v| v.as_str())
            .map(String::from),
        // Providers disagree on which claim carries a human-readable handle:
        // Keycloak/Authelia send `preferred_username`, Dex sends `name`.
        preferred_username: ["preferred_username", "name"]
            .iter()
            .find_map(|c| claims.get(*c).and_then(|v| v.as_str()))
            .map(String::from),
    })
}

/// Decode an id_token's claim set.
///
/// The signature is deliberately not verified. OpenID Connect Core §3.1.3.7
/// permits skipping id_token signature validation in the authorization code
/// flow when the token is obtained directly from the token endpoint over a
/// server-to-server TLS channel authenticated with the client secret, which is
/// exactly what `exchange_code` does. If this ever accepts tokens from any
/// other source, JWKS verification becomes mandatory.
fn decode_id_token_claims(id_token: &str) -> Result<serde_json::Value, AuthError> {
    let parts: Vec<&str> = id_token.split('.').collect();
    if parts.len() != 3 {
        return Err(AuthError::OidcError("malformed id_token".into()));
    }
    let payload_bytes = general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|e| AuthError::OidcError(format!("id_token base64 decode failed: {e}")))?;
    serde_json::from_slice(&payload_bytes)
        .map_err(|e| AuthError::OidcError(format!("id_token claims parse failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebase_swaps_internal_origin_for_public() {
        assert_eq!(
            rebase_endpoint(
                "http://dex:5556/dex/auth",
                "http://dex:5556/dex",
                "http://localhost:5556/dex"
            ),
            "http://localhost:5556/dex/auth"
        );
    }

    #[test]
    fn rebase_tolerates_trailing_slashes() {
        assert_eq!(
            rebase_endpoint(
                "http://dex:5556/dex/auth",
                "http://dex:5556/dex/",
                "http://localhost:5556/dex/"
            ),
            "http://localhost:5556/dex/auth"
        );
    }

    #[test]
    fn rebase_leaves_foreign_endpoints_untouched() {
        let endpoint = "https://elsewhere.example.com/authorize";
        assert_eq!(
            rebase_endpoint(endpoint, "http://dex:5556/dex", "http://localhost:5556/dex"),
            endpoint
        );
    }

    #[test]
    fn random_hex_is_the_requested_byte_length() {
        assert_eq!(random_hex(32).len(), 64);
        assert_ne!(random_hex(32), random_hex(32));
    }

    #[test]
    fn malformed_id_token_is_rejected() {
        assert!(decode_id_token_claims("not.a.jwt.at.all").is_err());
        assert!(decode_id_token_claims("onlyonepart").is_err());
    }

    #[test]
    fn id_token_claims_decode() {
        // {"sub":"abc","email":"a@b.c"}
        let payload = general_purpose::URL_SAFE_NO_PAD.encode(r#"{"sub":"abc","email":"a@b.c"}"#);
        let token = format!("header.{payload}.signature");
        let claims = decode_id_token_claims(&token).unwrap();
        assert_eq!(claims["sub"], "abc");
        assert_eq!(claims["email"], "a@b.c");
    }
}
