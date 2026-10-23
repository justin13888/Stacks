use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

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
    /// The PKCE (RFC 7636) verifier; its S256 challenge is in `auth_url`.
    /// Kept server-side and sent with the code exchange.
    pub pkce_verifier: String,
}

#[derive(Debug, Clone, Deserialize)]
struct OidcDiscovery {
    /// The provider's own canonical issuer. Endpoints are advertised relative
    /// to this, which may differ from the URL we fetched the document from.
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    id_token: Option<String>,
}

fn random_hex() -> String {
    hex::encode(super::random_bytes::<32>())
}

/// A PKCE verifier (43 base64url characters, the RFC 7636 minimum length).
fn pkce_verifier() -> String {
    general_purpose::URL_SAFE_NO_PAD.encode(super::random_bytes::<32>())
}

/// The S256 challenge for a PKCE verifier.
fn pkce_challenge(verifier: &str) -> String {
    use sha2::{Digest, Sha256};
    general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
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

/// Re-point a discovered endpoint at a different base URL.
///
/// A provider advertises every endpoint relative to its own configured issuer,
/// regardless of which host we fetched the discovery document from. In a
/// split-horizon deployment that means the two audiences need opposite
/// treatment: the authorization endpoint has to be reachable by the *browser*,
/// while the token endpoint has to be reachable by the *API*. `from` is
/// therefore the issuer the document itself declares, and `to` is whichever
/// base that particular endpoint's caller can reach.
///
/// Only the issuer prefix is swapped; the endpoint's own path suffix and query
/// are preserved.
fn rebase_endpoint(endpoint: &str, from: &str, to: &str) -> String {
    let from = from.trim_end_matches('/');
    let to = to.trim_end_matches('/');
    match endpoint.strip_prefix(from) {
        Some(suffix) => format!("{to}{suffix}"),
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

    let state = random_hex();
    let nonce = random_hex();
    let verifier = pkce_verifier();

    // The browser must be able to reach this one.
    let endpoint = rebase_endpoint(
        &discovery.authorization_endpoint,
        &discovery.issuer,
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
        .append_pair("nonce", &nonce)
        .append_pair("code_challenge", &pkce_challenge(&verifier))
        .append_pair("code_challenge_method", "S256");

    Ok(AuthorizationParams {
        auth_url: url.to_string(),
        state,
        nonce,
        pkce_verifier: verifier,
    })
}

/// Exchange an authorization code for tokens and extract user info.
///
/// `expected_nonce` and `pkce_verifier` come from the server-side
/// `oidc_login_states` row keyed by the `state` the provider echoed back — the
/// client never sees either, so neither can be forged. `state` itself is
/// validated by the caller before we get here.
pub async fn exchange_code(
    server_base: &str,
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
    code: &str,
    expected_nonce: &str,
    pkce_verifier: &str,
) -> Result<OidcUserInfo, AuthError> {
    let discovery = discover(server_base).await?;

    // This call is made by us, not the browser, so it must resolve on our side
    // of the split horizon.
    let token_endpoint = rebase_endpoint(&discovery.token_endpoint, &discovery.issuer, server_base);

    let client = reqwest::Client::new();
    let response = client
        .post(&token_endpoint)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code_verifier", pkce_verifier),
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

    /// The realistic split-horizon case: Dex is configured with a localhost
    /// issuer and advertises localhost endpoints, but the API reaches it as
    /// `dex:5556`. The browser-facing authorization endpoint stays on
    /// localhost; the server-to-server token endpoint must move to `dex`.
    #[test]
    fn rebase_sends_each_endpoint_to_its_own_audience() {
        let declared_issuer = "http://localhost:5556/dex";

        assert_eq!(
            rebase_endpoint(
                "http://localhost:5556/dex/auth",
                declared_issuer,
                "http://localhost:5556/dex",
            ),
            "http://localhost:5556/dex/auth",
            "browser-facing endpoint must stay publicly reachable"
        );

        assert_eq!(
            rebase_endpoint(
                "http://localhost:5556/dex/token",
                declared_issuer,
                "http://dex:5556/dex",
            ),
            "http://dex:5556/dex/token",
            "server-to-server endpoint must move onto the internal address"
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
    fn random_hex_is_32_bytes() {
        assert_eq!(random_hex().len(), 64);
        assert_ne!(random_hex(), random_hex());
    }

    /// RFC 7636 appendix B test vector.
    #[test]
    fn pkce_challenge_matches_the_rfc_vector() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        let v = pkce_verifier();
        assert_eq!(v.len(), 43);
        assert_ne!(v, pkce_verifier());
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
