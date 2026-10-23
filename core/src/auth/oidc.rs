//! OpenID Connect sign-in through the `openidconnect` crate.
//!
//! The authorization code flow with PKCE (S256). The id_token is verified in
//! full by `openidconnect`: its signature against the provider's JWKS, its
//! issuer, its audience (this client), its expiry, and the nonce bound to the
//! authorization request. Nothing about the identity is taken from an
//! unverified token.
//!
//! # Split-horizon providers
//!
//! A containerised deployment reaches the identity provider on one address
//! (`http://dex:5556/dex`) while the browser reaches it on another
//! (`http://localhost:5556/dex`). Discovery advertises every endpoint under the
//! provider's own issuer, so when [`OidcConfig::internal_issuer`] is set the
//! discovery document is fetched from the internal address, and the endpoints
//! Stacks itself calls (token, JWKS) are re-pointed at it. The authorization
//! endpoint, which the browser follows, keeps the public address, and the
//! issuer the id_token must carry is always the public one.

use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use openidconnect::core::{
    CoreAuthenticationFlow, CoreClient, CoreJsonWebKeySet, CoreProviderMetadata,
};
use openidconnect::{
    AsyncHttpClient, AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, HttpRequest,
    HttpResponse, IssuerUrl, JsonWebKeySetUrl, Nonce, PkceCodeChallenge, PkceCodeVerifier,
    RedirectUrl, RequestTokenError, Scope, TokenResponse, TokenUrl,
};

use crate::config::models::OidcConfig;

use super::AuthError;

/// How long a discovery document (and the JWKS fetched with it) is reused.
/// Providers rotate signing keys rarely and announce new ones ahead of use;
/// a key the cache does not know yet is picked up on the next refresh.
const DISCOVERY_TTL: Duration = Duration::from_secs(300);

/// The most of a provider response Stacks reads. Discovery documents, key
/// sets and token responses are a few KiB; anything near this is not one.
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

/// The identity an id_token proved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedIdentity {
    pub subject: String,
    pub email: Option<String>,
    /// Whether the provider vouched for `email` (`email_verified: true`).
    /// An absent claim counts as unverified: some providers let a user set
    /// any address.
    pub email_verified: bool,
    /// A human-readable handle: `preferred_username`, else `name`.
    pub preferred_username: Option<String>,
}

/// A pending authorization request: what the browser is sent to, and what
/// Stacks keeps to verify the callback.
#[derive(Debug, Clone)]
pub struct AuthorizationRequest {
    pub url: String,
    pub state: String,
    pub nonce: String,
    pub pkce_verifier: String,
}

/// A failed call to the identity provider, as `openidconnect` sees it.
#[derive(Debug, thiserror::Error)]
pub enum HttpFailure {
    #[error(transparent)]
    Transport(#[from] reqwest::Error),
    #[error("invalid HTTP message: {0}")]
    Message(#[from] openidconnect::http::Error),
    #[error("the response is larger than {MAX_RESPONSE_BYTES} bytes")]
    TooLarge,
}

/// The HTTP client `openidconnect` makes its calls through.
///
/// Redirects are never followed: OAuth 2.0 endpoints do not redirect, and
/// following one would let a compromised discovery document point Stacks at
/// an arbitrary internal URL.
#[derive(Debug, Clone)]
pub struct OidcHttp(reqwest::Client);

impl OidcHttp {
    pub fn new(timeout: Duration) -> Result<Self, AuthError> {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(timeout)
            .timeout(timeout)
            .build()
            .map(Self)
            .map_err(|e| AuthError::IdentityProvider(format!("HTTP client: {e}")))
    }
}

impl<'c> AsyncHttpClient<'c> for OidcHttp {
    type Error = HttpFailure;
    type Future = Pin<Box<dyn Future<Output = Result<HttpResponse, HttpFailure>> + Send + 'c>>;

    fn call(&'c self, request: HttpRequest) -> Self::Future {
        Box::pin(async move {
            let request = reqwest::Request::try_from(request)?;
            let mut response = self.0.execute(request).await?;
            let mut builder = openidconnect::http::Response::builder()
                .status(response.status())
                .version(response.version());
            for (name, value) in response.headers() {
                builder = builder.header(name, value);
            }
            let mut body = Vec::new();
            while let Some(chunk) = response.chunk().await? {
                if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
                    return Err(HttpFailure::TooLarge);
                }
                body.extend_from_slice(&chunk);
            }
            Ok(builder.body(body)?)
        })
    }
}

/// The configured identity provider, with its discovery document cached.
pub struct OidcProvider {
    http: OidcHttp,
    config: OidcConfig,
    cached: Mutex<Option<(Instant, CoreProviderMetadata)>>,
}

impl OidcProvider {
    pub fn new(config: OidcConfig, http: OidcHttp) -> Self {
        Self {
            http,
            config,
            cached: Mutex::new(None),
        }
    }

    /// The public issuer accounts are keyed by.
    pub fn issuer(&self) -> &str {
        &self.config.issuer
    }

    /// Start an authorization request: the URL to send the browser to, and
    /// the state, nonce and PKCE verifier to keep server-side.
    pub async fn authorization_request(&self) -> Result<AuthorizationRequest, AuthError> {
        let client = self.client().await?;
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let (url, state, nonce) = client
            .authorize_url(
                CoreAuthenticationFlow::AuthorizationCode,
                CsrfToken::new_random,
                Nonce::new_random,
            )
            .add_scope(Scope::new("email".to_owned()))
            .add_scope(Scope::new("profile".to_owned()))
            .set_pkce_challenge(challenge)
            .url();
        Ok(AuthorizationRequest {
            url: url.to_string(),
            state: state.into_secret(),
            nonce: nonce.secret().clone(),
            pkce_verifier: verifier.into_secret(),
        })
    }

    /// Exchange an authorization code and verify the id_token it yields.
    ///
    /// `nonce` and `pkce_verifier` are the ones stored for the request's
    /// `state`, so neither ever passes through the browser.
    pub async fn exchange(
        &self,
        code: &str,
        nonce: &str,
        pkce_verifier: &str,
    ) -> Result<VerifiedIdentity, AuthError> {
        let client = self.client().await?;
        let request = client
            .exchange_code(AuthorizationCode::new(code.to_owned()))
            .map_err(|e| AuthError::IdentityProvider(format!("no token endpoint: {e}")))?
            .set_pkce_verifier(PkceCodeVerifier::new(pkce_verifier.to_owned()));
        let token = request
            .request_async(&self.http)
            .await
            .map_err(|e| match e {
                // The provider understood the request and refused it: an expired,
                // reused or forged code, or a PKCE mismatch.
                RequestTokenError::ServerResponse(refusal) => AuthError::LoginRejected(format!(
                    "the identity provider refused the code: {refusal}"
                )),
                RequestTokenError::Request(failure) => {
                    AuthError::IdentityProvider(format!("token request failed: {failure}"))
                }
                other => AuthError::IdentityProvider(format!("unusable token response: {other}")),
            })?;
        let id_token = token
            .id_token()
            .ok_or_else(|| AuthError::LoginRejected("the token response has no id_token".into()))?;
        let verifier = client.id_token_verifier();
        let claims = id_token
            .claims(&verifier, &Nonce::new(nonce.to_owned()))
            .map_err(|e| {
                AuthError::LoginRejected(format!("the id_token failed verification: {e}"))
            })?;

        let preferred_username = claims
            .preferred_username()
            .map(|u| u.as_str().to_owned())
            .or_else(|| {
                claims
                    .name()
                    .and_then(|name| name.get(None))
                    .map(|n| n.as_str().to_owned())
            });
        Ok(VerifiedIdentity {
            subject: claims.subject().as_str().to_owned(),
            email: claims.email().map(|e| e.as_str().to_owned()),
            email_verified: claims.email_verified() == Some(true),
            preferred_username,
        })
    }

    async fn client(
        &self,
    ) -> Result<
        CoreClient<
            openidconnect::EndpointSet,
            openidconnect::EndpointNotSet,
            openidconnect::EndpointNotSet,
            openidconnect::EndpointNotSet,
            openidconnect::EndpointMaybeSet,
            openidconnect::EndpointMaybeSet,
        >,
        AuthError,
    > {
        let metadata = self.metadata().await?;
        let redirect = RedirectUrl::new(self.config.redirect_uri.clone())
            .map_err(|e| AuthError::IdentityProvider(format!("invalid redirect URI: {e}")))?;
        Ok(CoreClient::from_provider_metadata(
            metadata,
            ClientId::new(self.config.client_id.clone()),
            Some(ClientSecret::new(self.config.client_secret.clone())),
        )
        .set_redirect_uri(redirect))
    }

    /// The provider's metadata with its key set, from the cache when fresh.
    async fn metadata(&self) -> Result<CoreProviderMetadata, AuthError> {
        if let Some(metadata) = self.fresh() {
            return Ok(metadata);
        }
        let metadata = self.discover().await?;
        if let Ok(mut cached) = self.cached.lock() {
            *cached = Some((Instant::now(), metadata.clone()));
        }
        Ok(metadata)
    }

    fn fresh(&self) -> Option<CoreProviderMetadata> {
        let cached = self.cached.lock().ok()?;
        let (fetched, metadata) = cached.as_ref()?;
        (fetched.elapsed() < DISCOVERY_TTL).then(|| metadata.clone())
    }

    async fn discover(&self) -> Result<CoreProviderMetadata, AuthError> {
        let public = IssuerUrl::new(self.config.issuer.clone())
            .map_err(|e| AuthError::IdentityProvider(format!("invalid issuer URL: {e}")))?;
        let server_base = self.config.server_base().trim_end_matches('/');
        let url = format!("{server_base}/.well-known/openid-configuration");
        let request = openidconnect::http::Request::builder()
            .uri(url)
            .header(openidconnect::http::header::ACCEPT, "application/json")
            .body(Vec::new())
            .map_err(|e| AuthError::IdentityProvider(format!("discovery request: {e}")))?;
        let response = self
            .http
            .call(request)
            .await
            .map_err(|e| AuthError::IdentityProvider(format!("discovery failed: {e}")))?;
        if !response.status().is_success() {
            return Err(AuthError::IdentityProvider(format!(
                "discovery returned {}",
                response.status()
            )));
        }
        let mut metadata: CoreProviderMetadata =
            serde_json::from_slice(response.body()).map_err(|e| {
                AuthError::IdentityProvider(format!("unusable discovery document: {e}"))
            })?;

        // OpenID Connect Discovery 1.0 section 4.3: the document must name the
        // issuer it was requested for. That is the public issuer whatever
        // address it was fetched from, because id_tokens carry it.
        if metadata.issuer().as_str().trim_end_matches('/') != public.as_str().trim_end_matches('/')
        {
            return Err(AuthError::IdentityProvider(format!(
                "the discovery document names issuer {}, not {}",
                metadata.issuer().as_str(),
                public.as_str()
            )));
        }

        let declared = metadata.issuer().as_str().to_owned();
        let authorization = rebase(
            metadata.authorization_endpoint().as_str(),
            &declared,
            &self.config.issuer,
        );
        metadata = metadata
            .set_authorization_endpoint(AuthUrl::new(authorization).map_err(|e| {
                AuthError::IdentityProvider(format!("authorization endpoint: {e}"))
            })?);
        if let Some(token) = metadata.token_endpoint().cloned() {
            let rebased = rebase(token.as_str(), &declared, server_base);
            metadata =
                metadata
                    .set_token_endpoint(Some(TokenUrl::new(rebased).map_err(|e| {
                        AuthError::IdentityProvider(format!("token endpoint: {e}"))
                    })?));
        }
        let jwks_uri =
            JsonWebKeySetUrl::new(rebase(metadata.jwks_uri().as_str(), &declared, server_base))
                .map_err(|e| AuthError::IdentityProvider(format!("jwks_uri: {e}")))?;
        let jwks = CoreJsonWebKeySet::fetch_async(&jwks_uri, &self.http)
            .await
            .map_err(|e| AuthError::IdentityProvider(format!("key set fetch failed: {e}")))?;
        Ok(metadata.set_jwks_uri(jwks_uri).set_jwks(jwks))
    }
}

/// Re-point a discovered endpoint from the provider's declared issuer onto
/// `to`, keeping its path suffix and query. An endpoint outside the declared
/// issuer is left alone rather than mangled.
fn rebase(endpoint: &str, from: &str, to: &str) -> String {
    let from = from.trim_end_matches('/');
    let to = to.trim_end_matches('/');
    match endpoint.strip_prefix(from) {
        Some(suffix) => format!("{to}{suffix}"),
        None => endpoint.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dex configured with a localhost issuer, reached by the API as
    /// `dex:5556`: the browser keeps localhost, the API moves to `dex`.
    #[test]
    fn rebase_sends_each_endpoint_to_its_own_audience() {
        let declared = "http://localhost:5556/dex";
        assert_eq!(
            rebase(
                "http://localhost:5556/dex/auth",
                declared,
                "http://localhost:5556/dex"
            ),
            "http://localhost:5556/dex/auth"
        );
        assert_eq!(
            rebase(
                "http://localhost:5556/dex/token",
                declared,
                "http://dex:5556/dex"
            ),
            "http://dex:5556/dex/token"
        );
    }

    #[test]
    fn rebase_tolerates_trailing_slashes() {
        assert_eq!(
            rebase(
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
            rebase(endpoint, "http://dex:5556/dex", "http://localhost:5556/dex"),
            endpoint
        );
    }
}
