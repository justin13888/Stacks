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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub bind: String,
    pub log_level: String,
    pub data_dir: String,
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
}
