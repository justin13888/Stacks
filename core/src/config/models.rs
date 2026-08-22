use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
    pub server: ServerConfig,
    pub cameo: CameoConfig,
    pub beam: BeamConfig,
    pub oidc: OidcConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Secret key for JWT signing and AES-256-GCM encryption.
    /// Must be at least 32 bytes / 64 hex chars.
    pub secret_key: String,
    /// JWT access token lifetime in seconds (default: 900 = 15 min).
    pub access_token_ttl_secs: u64,
    /// Refresh token lifetime in seconds (default: 2592000 = 30 days).
    pub refresh_token_ttl_secs: u64,
    /// Email addresses provisioned (or promoted) to the `admin` role on login.
    ///
    /// Without this there is no way to obtain an admin account at all: OIDC
    /// always provisions `role = "user"`, so every `/admin/*` route — and with
    /// it the whole streaming-server surface — would be permanently unreachable.
    pub admin_emails: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub bind: String,
    pub log_level: String,
    pub data_dir: String,
    /// CORS allowlist for the API. An empty list falls back to localhost
    /// developer defaults; a single `*` entry opens to any origin.
    pub cors_allowed_origins: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameoConfig {
    pub tmdb_api_key: String,
    pub cache_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeamConfig {
    /// How often to poll Beam for library changes (seconds, default: 3600).
    pub sync_interval_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcConfig {
    /// OIDC discovery URL (e.g. https://auth.example.com/.well-known/openid-configuration)
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    /// Redirect URI registered in the OIDC provider.
    pub redirect_uri: String,
    /// Optional server-to-server base URL for the same provider.
    ///
    /// Containerised deployments hit the split-horizon problem: the browser
    /// reaches the IdP on a published host (`http://localhost:5556/dex`) while
    /// the API reaches it on the compose network (`http://dex:5556/dex`).
    /// Discovery returns absolute URLs, so exactly one of the two would break.
    /// When set, discovery and token exchange use this URL and the discovered
    /// `authorization_endpoint` is rewritten back onto `issuer` before being
    /// handed to the browser. The same shape is needed behind a reverse proxy.
    pub internal_issuer: Option<String>,
}

impl OidcConfig {
    /// Base URL to use for server-to-server calls (discovery, token exchange).
    pub fn server_base(&self) -> &str {
        self.internal_issuer.as_deref().unwrap_or(&self.issuer)
    }
}
