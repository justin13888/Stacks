use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
    pub server: ServerConfig,
    pub cameo: CameoConfig,
    pub imports: ImportConfig,
    pub oidc: OidcConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportConfig {
    /// Largest import upload accepted, in bytes (default 10 MiB). The original
    /// file is kept in `imports.original_file`, so this also bounds storage.
    pub max_file_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Secret key for JWT signing. Must be at least 32 bytes.
    pub secret_key: String,
    /// JWT access token lifetime in seconds (default: 900 = 15 min).
    pub access_token_ttl_secs: u64,
    /// Refresh token lifetime in seconds (default: 2592000 = 30 days).
    pub refresh_token_ttl_secs: u64,
    /// Email addresses provisioned (or promoted) to the `admin` role on login.
    ///
    /// Without this there is no way to obtain an admin account at all: OIDC
    /// always provisions `role = "user"`, so every `/admin/*` route would be
    /// permanently unreachable.
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
    /// Per-provider TCP + TLS connect timeout, in seconds (default 3).
    pub connect_timeout_secs: u64,
    /// Per-attempt request timeout (connect through full body), in seconds
    /// (default 10).
    pub request_timeout_secs: u64,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn oidc(internal: Option<&str>) -> OidcConfig {
        OidcConfig {
            issuer: "http://localhost:5556/dex".into(),
            client_id: "stacks".into(),
            client_secret: "s".into(),
            redirect_uri: "http://localhost:3000/auth/callback".into(),
            internal_issuer: internal.map(String::from),
        }
    }

    #[test]
    fn server_base_defaults_to_the_public_issuer() {
        assert_eq!(oidc(None).server_base(), "http://localhost:5556/dex");
    }

    #[test]
    fn server_base_prefers_the_internal_issuer() {
        assert_eq!(
            oidc(Some("http://dex:5556/dex")).server_base(),
            "http://dex:5556/dex"
        );
    }
}
