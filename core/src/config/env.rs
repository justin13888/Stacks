use std::env;

use super::{
    ConfigError,
    file::FileConfig,
    models::{
        AuthConfig, CameoConfig, Config, DatabaseConfig, ImportConfig, OidcConfig, ServerConfig,
    },
};

/// Required value: environment variable, else config file, else error.
fn require(key: &'static str, file: Option<String>) -> Result<String, ConfigError> {
    env::var(key)
        .ok()
        .or(file)
        .ok_or(ConfigError::MissingEnvVar(key))
}

/// Optional string: environment variable, else config file, else default.
fn optional(key: &str, file: Option<String>, default: &str) -> String {
    env::var(key)
        .ok()
        .or(file)
        .unwrap_or_else(|| default.to_string())
}

/// Optional string with no default: environment variable, else config file.
/// An empty value counts as unset.
fn optional_opt(key: &str, file: Option<String>) -> Option<String> {
    env::var(key).ok().or(file).filter(|v| !v.trim().is_empty())
}

/// Optional u64: environment variable, else config file, else default.
fn optional_u64(key: &str, file: Option<u64>, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .or(file)
        .unwrap_or(default)
}

/// Parse a comma-separated list from the env, falling back to the file value,
/// then to an empty list.
fn parse_csv(env_value: Option<&str>, file_value: Option<Vec<String>>) -> Vec<String> {
    if let Some(raw) = env_value {
        return raw
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    file_value.unwrap_or_default()
}

/// Reject a CORS allowlist that would expose sessions to every site.
///
/// The session is a cookie, so the API answers cross-origin requests with
/// credentials; the Fetch standard forbids that for a wildcard origin, and
/// echoing any origin instead would let every site act as the signed-in user.
fn validate_cors(origins: &[String]) -> Result<(), ConfigError> {
    if origins.iter().any(|o| o == "*") {
        return Err(ConfigError::Invalid(
            "WW_CORS_ALLOWED_ORIGINS cannot be `*`: sessions are cookies, so list each origin"
                .to_string(),
        ));
    }
    Ok(())
}

/// Build the configuration, layering environment variables over `config.toml`.
pub fn build(file: FileConfig) -> Result<Config, ConfigError> {
    let data_dir = optional("WW_DATA_DIR", file.server.data_dir, "/data");
    let cors_allowed_origins = parse_csv(
        env::var("WW_CORS_ALLOWED_ORIGINS").ok().as_deref(),
        file.server.cors_allowed_origins,
    );
    validate_cors(&cors_allowed_origins)?;
    Ok(Config {
        database: DatabaseConfig {
            url: require("WW_DATABASE_URL", file.database.url)?,
        },
        auth: AuthConfig {
            session_ttl_secs: optional_u64(
                "WW_SESSION_TTL_SECS",
                file.auth.session_ttl_secs,
                2_592_000,
            ),
            admin_emails: parse_csv(
                env::var("WW_ADMIN_EMAILS").ok().as_deref(),
                file.auth.admin_emails,
            )
            .into_iter()
            .map(|e| e.to_ascii_lowercase())
            .collect(),
        },
        server: ServerConfig {
            bind: optional("WW_BIND", file.server.bind, "0.0.0.0:8095"),
            log_level: optional("WW_LOG_LEVEL", file.server.log_level, "info"),
            data_dir: data_dir.clone(),
            cors_allowed_origins,
            metrics_bind: optional("WW_METRICS_BIND", file.server.metrics_bind, "0.0.0.0:9095"),
            metrics_token: optional_opt("WW_METRICS_TOKEN", file.server.metrics_token),
            trusted_proxy_hops: optional_u64(
                "WW_TRUSTED_PROXY_HOPS",
                file.server.trusted_proxy_hops,
                1,
            ),
            rate_limit_per_minute: optional_u64(
                "WW_RATE_LIMIT_PER_MINUTE",
                file.server.rate_limit_per_minute,
                600,
            ),
        },
        cameo: CameoConfig {
            tmdb_api_key: require("WW_TMDB_API_KEY", file.cameo.tmdb_api_key)?,
            cache_dir: optional(
                "WW_CAMEO_CACHE_DIR",
                file.cameo.cache_dir,
                &format!("{data_dir}/cameo"),
            ),
            connect_timeout_secs: optional_u64(
                "WW_METADATA_CONNECT_TIMEOUT_SECS",
                file.cameo.connect_timeout_secs,
                3,
            ),
            request_timeout_secs: optional_u64(
                "WW_METADATA_REQUEST_TIMEOUT_SECS",
                file.cameo.request_timeout_secs,
                10,
            ),
            tmdb_base_url: optional_opt("WW_TMDB_BASE_URL", file.cameo.tmdb_base_url),
            anilist_base_url: optional_opt("WW_ANILIST_BASE_URL", file.cameo.anilist_base_url),
        },
        imports: ImportConfig {
            max_file_bytes: optional_u64(
                "WW_IMPORT_MAX_BYTES",
                file.imports.max_file_bytes,
                10 * 1024 * 1024,
            ),
        },
        oidc: OidcConfig {
            issuer: require("WW_OIDC_ISSUER", file.oidc.issuer)?,
            client_id: require("WW_OIDC_CLIENT_ID", file.oidc.client_id)?,
            client_secret: require("WW_OIDC_CLIENT_SECRET", file.oidc.client_secret)?,
            redirect_uri: require("WW_OIDC_REDIRECT_URI", file.oidc.redirect_uri)?,
            internal_issuer: optional_opt("WW_OIDC_INTERNAL_ISSUER", file.oidc.internal_issuer),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wildcard_cors_origin_is_rejected() {
        assert!(validate_cors(&["https://app.example".into()]).is_ok());
        assert!(validate_cors(&[]).is_ok());
        let err = validate_cors(&["*".into()]).unwrap_err();
        assert!(err.to_string().contains("cannot be `*`"));
    }

    #[test]
    fn csv_parsing_prefers_env_and_trims() {
        assert_eq!(
            parse_csv(Some("a@x.com, b@y.com ,"), None),
            vec!["a@x.com", "b@y.com"]
        );
        // Env wins over the file value.
        assert_eq!(
            parse_csv(Some("only"), Some(vec!["ignored".into()])),
            vec!["only"]
        );
        // Falls back to the file value, then to empty.
        assert_eq!(parse_csv(None, Some(vec!["f".into()])), vec!["f"]);
        assert!(parse_csv(None, None).is_empty());
    }
}
