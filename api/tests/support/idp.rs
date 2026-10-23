//! A mock OpenID Connect identity provider on wiremock.
//!
//! It publishes a discovery document and a JWKS, and answers the token
//! endpoint with an RS256 id_token. The token endpoint checks the PKCE
//! verifier against the challenge the authorization URL carried, so a test
//! that drives the real sign-in exercises PKCE end to end.

use std::sync::{Arc, Mutex, OnceLock};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{Duration, Utc};
use openidconnect::core::{
    CoreIdToken, CoreIdTokenClaims, CoreJsonWebKeySet, CoreJwsSigningAlgorithm,
    CoreRsaPrivateSigningKey,
};
use openidconnect::{
    Audience, EmptyAdditionalClaims, EndUserEmail, EndUserUsername, IssuerUrl, JsonWebKeyId, Nonce,
    PrivateSigningKey, StandardClaims, SubjectIdentifier,
};
use rsa::pkcs1::EncodeRsaPrivateKey;
use sha2::Digest;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

pub const CLIENT_ID: &str = "stacks-test";
pub const CLIENT_SECRET: &str = "stacks-test-secret";
pub const REDIRECT_URI: &str = "http://localhost:3000/auth/callback";

/// A 2048-bit key, made once per test binary: generating one is slow in a
/// debug build.
fn key_pem(slot: usize) -> &'static str {
    static KEYS: OnceLock<[String; 2]> = OnceLock::new();
    let keys = KEYS.get_or_init(|| {
        let make = || {
            rsa::RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048)
                .expect("an RSA key")
                .to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)
                .expect("PEM")
                .to_string()
        };
        [make(), make()]
    });
    &keys[slot]
}

fn signing_key(slot: usize) -> CoreRsaPrivateSigningKey {
    CoreRsaPrivateSigningKey::from_pem(key_pem(slot), Some(JsonWebKeyId::new(format!("k{slot}"))))
        .expect("a signing key")
}

/// What the next id_token the provider issues says, and how it is signed.
#[derive(Clone, Debug)]
pub struct NextLogin {
    pub subject: String,
    pub email: Option<String>,
    pub username: Option<String>,
    /// Sign with a key the JWKS does not publish.
    pub foreign_key: bool,
    /// Issue it for another client.
    pub wrong_audience: bool,
    /// Echo this nonce instead of the one the authorization request carried.
    pub nonce_override: Option<String>,
    /// Answer the token request with a 503 page instead of tokens.
    pub token_endpoint_down: bool,
}

impl NextLogin {
    pub fn subject(subject: &str) -> Self {
        Self {
            subject: subject.to_owned(),
            email: Some(format!("{subject}@example.test")),
            username: Some(subject.to_owned()),
            foreign_key: false,
            wrong_audience: false,
            nonce_override: None,
            token_endpoint_down: false,
        }
    }
}

/// One authorization request the test is completing.
#[derive(Clone, Debug, Default)]
struct Pending {
    nonce: String,
    challenge: String,
}

#[derive(Default)]
struct State {
    pending: Option<Pending>,
    next: Option<NextLogin>,
}

pub struct MockIdp {
    pub server: MockServer,
    state: Arc<Mutex<State>>,
}

struct TokenEndpoint {
    issuer: String,
    state: Arc<Mutex<State>>,
}

fn form(body: &[u8]) -> std::collections::HashMap<String, String> {
    url::form_urlencoded::parse(body).into_owned().collect()
}

impl Respond for TokenEndpoint {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let fields = form(&request.body);
        let state = self.state.lock().expect("idp state");
        let (Some(pending), Some(next)) = (state.pending.clone(), state.next.clone()) else {
            return invalid_grant();
        };
        if next.token_endpoint_down {
            return ResponseTemplate::new(503).set_body_string("<html>down for maintenance</html>");
        }
        // RFC 7636 section 4.6: S256(verifier) must equal the challenge.
        let verifier = fields.get("code_verifier").cloned().unwrap_or_default();
        let challenge = URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(verifier.as_bytes()));
        if challenge != pending.challenge
            || fields.get("code").map(String::as_str) != Some("good-code")
        {
            return invalid_grant();
        }
        let audience = if next.wrong_audience {
            "someone-else"
        } else {
            CLIENT_ID
        };
        let mut standard = StandardClaims::new(SubjectIdentifier::new(next.subject.clone()));
        if let Some(email) = &next.email {
            standard = standard.set_email(Some(EndUserEmail::new(email.clone())));
        }
        if let Some(username) = &next.username {
            standard =
                standard.set_preferred_username(Some(EndUserUsername::new(username.clone())));
        }
        let nonce = next.nonce_override.clone().unwrap_or(pending.nonce);
        let claims = CoreIdTokenClaims::new(
            IssuerUrl::new(self.issuer.clone()).expect("issuer"),
            vec![Audience::new(audience.to_owned())],
            Utc::now() + Duration::minutes(5),
            Utc::now(),
            standard,
            EmptyAdditionalClaims {},
        )
        .set_nonce(Some(Nonce::new(nonce)));
        let key = signing_key(usize::from(next.foreign_key));
        let token = CoreIdToken::new(
            claims,
            &key,
            CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256,
            None,
            None,
        )
        .expect("an id_token");
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "an-access-token",
            "token_type": "Bearer",
            "expires_in": 300,
            "id_token": token.to_string(),
        }))
    }
}

fn invalid_grant() -> ResponseTemplate {
    ResponseTemplate::new(400).set_body_json(serde_json::json!({ "error": "invalid_grant" }))
}

impl MockIdp {
    pub async fn start() -> Self {
        let server = MockServer::start().await;
        let issuer = server.uri();
        let state = Arc::new(Mutex::new(State::default()));
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issuer": issuer,
                "authorization_endpoint": format!("{issuer}/auth"),
                "token_endpoint": format!("{issuer}/token"),
                "jwks_uri": format!("{issuer}/keys"),
                "response_types_supported": ["code"],
                "subject_types_supported": ["public"],
                "id_token_signing_alg_values_supported": ["RS256"],
            })))
            .mount(&server)
            .await;
        let jwks = CoreJsonWebKeySet::new(vec![signing_key(0).as_verification_key()]);
        Mock::given(method("GET"))
            .and(path("/keys"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&jwks))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(TokenEndpoint {
                issuer: issuer.clone(),
                state: Arc::clone(&state),
            })
            .mount(&server)
            .await;
        Self { server, state }
    }

    pub fn issuer(&self) -> String {
        self.server.uri()
    }

    /// Read the nonce and PKCE challenge out of an authorization URL the API
    /// issued, as the provider would on receiving the browser, and decide
    /// what the id_token it then issues says. Returns the request's `state`.
    pub fn authorize(&self, authorization_url: &str, next: NextLogin) -> String {
        let url = reqwest::Url::parse(authorization_url).expect("an authorization URL");
        let query: std::collections::HashMap<String, String> =
            url.query_pairs().into_owned().collect();
        assert_eq!(query.get("client_id").map(String::as_str), Some(CLIENT_ID));
        assert_eq!(
            query.get("code_challenge_method").map(String::as_str),
            Some("S256")
        );
        let mut state = self.state.lock().expect("idp state");
        state.pending = Some(Pending {
            nonce: query.get("nonce").cloned().expect("a nonce"),
            challenge: query
                .get("code_challenge")
                .cloned()
                .expect("a PKCE challenge"),
        });
        state.next = Some(next);
        query.get("state").cloned().expect("a state")
    }
}
