use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
    pub server: ServerConfig,
    pub cameo: CameoConfig,
    pub imports: ImportConfig,
    pub jobs: JobsConfig,
    pub oidc: OidcConfig,
    pub notifications: NotificationsConfig,
}

/// Where notification channels may deliver.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationsConfig {
    /// Host names and CIDRs whose private-network addresses (RFC 1918,
    /// `100.64.0.0/10`, `fc00::/7`) a channel may deliver to, such as a
    /// self-hosted Apprise or ntfy on the LAN. Empty (the default) permits
    /// public addresses only. Loopback, link-local and the like are refused
    /// whatever this says. Each entry is valid: [`build`](super::env::build)
    /// checks it.
    pub allowed_hosts: Vec<String>,
}

impl NotificationsConfig {
    /// The address policy deliveries are held to.
    pub fn address_policy(&self) -> crate::notifications::channels::AddressPolicy {
        crate::notifications::channels::AddressPolicy::from_allowed_hosts(&self.allowed_hosts)
            .unwrap_or_default()
    }
}

/// How long finished background jobs are kept before the job runner's daily
/// prune deletes them. Jobs are operational records, not user data; pending
/// and running jobs are never pruned. `0` keeps that status forever.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobsConfig {
    /// Days a completed job is kept (default 30).
    pub completed_retention_days: u64,
    /// Days a failed job is kept (default 90): longer, so an operator can
    /// still read why it failed, or retry it, well after the fact.
    pub failed_retention_days: u64,
}

impl Default for JobsConfig {
    fn default() -> Self {
        Self {
            completed_retention_days: 30,
            failed_retention_days: 90,
        }
    }
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
    /// How long a sign-in session lives, in seconds (default: 2592000 = 30
    /// days). Sessions are opaque server-side rows; ending one is immediate.
    pub session_ttl_secs: u64,
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
    /// Origins allowed to call the API from a browser with credentials. Empty
    /// (the default) mounts no CORS policy at all, which is right for the
    /// supported topology: the web app reaches the API through its own origin.
    pub cors_allowed_origins: Vec<String>,
    /// Where the operator-only listener (`/metrics`) binds. Empty disables it.
    pub metrics_bind: String,
    /// The bearer token `/metrics` demands. Unset serves it to anyone who can
    /// reach `metrics_bind`.
    pub metrics_token: Option<String>,
    /// How many reverse proxies in front of the API append to
    /// `X-Forwarded-For` (default 1: the web container's nginx). The rate
    /// limiter keys on the client address they report; trusting more hops than
    /// exist lets a client choose its own bucket.
    pub trusted_proxy_hops: u64,
    /// Requests one client may make per minute (default 600).
    pub rate_limit_per_minute: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameoConfig {
    pub tmdb_api_key: String,
    /// Per-provider TCP + TLS connect timeout, in seconds (default 3).
    pub connect_timeout_secs: u64,
    /// Per-attempt request timeout (connect through full body), in seconds
    /// (default 10).
    pub request_timeout_secs: u64,
    /// Where TMDB is reached instead of `https://api.themoviedb.org`. Unset in
    /// production; the end-to-end suite points it at its fixture server.
    pub tmdb_base_url: Option<String>,
    /// Where AniList's GraphQL endpoint is reached instead of
    /// `https://graphql.anilist.co`. Unset in production, like
    /// `tmdb_base_url`.
    pub anilist_base_url: Option<String>,
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
