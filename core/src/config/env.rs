use std::env;

use super::{
    ConfigError,
    file::FileConfig,
    models::{
        AuthConfig, CameoConfig, Config, DatabaseConfig, ImportConfig, JobsConfig,
        NotificationsConfig, OidcConfig, ServerConfig,
    },
};
use crate::notifications::channels::AddressPolicy;

/// Required value: environment variable, else config file, else error.
fn require(key: &'static str, file: Option<String>) -> Result<String, ConfigError> {
    env::var(key)
        .ok()
        .or(file)
        .ok_or(ConfigError::MissingEnvVar(key))
}

/// An optional variable's environment value, where an empty one counts as
/// unset so that the config file's value still applies.
///
/// Compose passes optional variables through as `${VAR:-}`, so an unset one
/// reaches the container as an empty string; reading that as a value would
/// hide the mounted config file's setting.
fn env_value(key: &str) -> Option<String> {
    non_empty(env::var(key).ok())
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.trim().is_empty())
}

/// Optional string: environment variable, else config file, else default.
fn optional(key: &str, file: Option<String>, default: &str) -> String {
    env_value(key)
        .or(file)
        .unwrap_or_else(|| default.to_string())
}

/// Optional string with no default: environment variable, else config file.
/// An empty value counts as unset in either layer.
fn optional_opt(key: &str, file: Option<String>) -> Option<String> {
    non_empty(env_value(key).or(file))
}

/// Optional u64: environment variable, else config file, else default.
fn optional_u64(key: &str, file: Option<u64>, default: u64) -> u64 {
    env_value(key)
        .and_then(|v| v.parse().ok())
        .or(file)
        .unwrap_or(default)
}

/// The operator listener's address. Unlike every other optional variable, an
/// explicitly empty `WW_METRICS_BIND` is a value: it disables the listener,
/// even over the config file's address.
fn metrics_bind(env_value: Option<String>, file: Option<String>) -> String {
    env_value
        .or(file)
        .unwrap_or_else(|| "0.0.0.0:9095".to_string())
}

/// Parse a comma-separated list from the env, falling back to the file value,
/// then to an empty list. An empty env value counts as unset.
fn parse_csv(env_value: Option<&str>, file_value: Option<Vec<String>>) -> Vec<String> {
    if let Some(raw) = env_value.filter(|v| !v.trim().is_empty()) {
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
    let cors_allowed_origins = parse_csv(
        env::var("WW_CORS_ALLOWED_ORIGINS").ok().as_deref(),
        file.server.cors_allowed_origins,
    );
    validate_cors(&cors_allowed_origins)?;
    let notifications = NotificationsConfig {
        allowed_hosts: parse_csv(
            env::var("WW_NOTIFICATION_ALLOWED_HOSTS").ok().as_deref(),
            file.notifications.allowed_hosts,
        ),
    };
    AddressPolicy::from_allowed_hosts(&notifications.allowed_hosts)
        .map_err(|e| ConfigError::Invalid(format!("WW_NOTIFICATION_ALLOWED_HOSTS: {e}")))?;
    Ok(Config {
        notifications,
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
            cors_allowed_origins,
            metrics_bind: metrics_bind(env::var("WW_METRICS_BIND").ok(), file.server.metrics_bind),
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
        jobs: JobsConfig {
            completed_retention_days: optional_u64(
                "WW_JOB_RETENTION_DAYS",
                file.jobs.completed_retention_days,
                JobsConfig::default().completed_retention_days,
            ),
            failed_retention_days: optional_u64(
                "WW_FAILED_JOB_RETENTION_DAYS",
                file.jobs.failed_retention_days,
                JobsConfig::default().failed_retention_days,
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
    fn a_setting_prefers_env_then_file_then_default() {
        // `PATH` is set in every test environment; the other key never is.
        // It deliberately lacks the `WW_` prefix: `scripts/check-env-docs`
        // treats every quoted `WW_*` literal here as a documented setting.
        let path = env::var("PATH").expect("PATH is set");
        assert_eq!(optional("PATH", Some("file".into()), "default"), path);
        let unset = "STACKS_TEST_NEVER_SET_LOG_LEVEL";
        assert_eq!(optional(unset, Some("debug".into()), "info"), "debug");
        assert_eq!(optional(unset, None, "info"), "info");
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
        // An empty env value is unset, so the file value still applies.
        assert_eq!(parse_csv(Some(""), Some(vec!["f".into()])), vec!["f"]);
        assert_eq!(parse_csv(Some(" "), Some(vec!["f".into()])), vec!["f"]);
    }

    #[test]
    fn an_empty_env_value_does_not_hide_the_file_value() {
        // Compose's `${VAR:-}` hands an unset variable over as "".
        assert_eq!(non_empty(Some(String::new())), None);
        assert_eq!(non_empty(Some("  ".into())), None);
        assert_eq!(non_empty(Some("x".into())).as_deref(), Some("x"));
        // `env_value` applies that filter to the real environment.
        let unset = "STACKS_TEST_NEVER_SET_BASE_URL";
        assert_eq!(env_value(unset), None);
        let file = || Some("http://file.example".to_string());
        assert_eq!(optional_opt(unset, file()), file());
        assert_eq!(optional_opt(unset, Some(" ".into())), None);
        assert_eq!(optional(unset, file(), "default"), "http://file.example");
        assert_eq!(optional_u64(unset, Some(7), 1), 7);
    }

    /// A config file holding every required value, and `allowed_hosts`.
    fn file_with(allowed_hosts: &[&str]) -> FileConfig {
        let mut file = FileConfig::default();
        file.database.url = Some("postgres://db".into());
        file.cameo.tmdb_api_key = Some("k".into());
        file.oidc.issuer = Some("https://idp.example".into());
        file.oidc.client_id = Some("stacks".into());
        file.oidc.client_secret = Some("s".into());
        file.oidc.redirect_uri = Some("https://stacks.example/auth/callback".into());
        file.notifications.allowed_hosts =
            Some(allowed_hosts.iter().map(|h| (*h).to_owned()).collect());
        file
    }

    #[test]
    fn the_notification_allow_list_is_checked_when_loaded() {
        let config = build(file_with(&["apprise.lan", "10.0.0.0/8"])).unwrap();
        assert_eq!(
            config.notifications.allowed_hosts,
            ["apprise.lan", "10.0.0.0/8"]
        );
        assert!(
            config
                .notifications
                .address_policy()
                .permits(None, "10.1.2.3".parse().unwrap())
        );
        let err = build(file_with(&["10.0.0.0/33"])).unwrap_err();
        assert!(
            err.to_string().contains("WW_NOTIFICATION_ALLOWED_HOSTS"),
            "{err}"
        );
    }

    #[test]
    fn an_empty_metrics_bind_disables_the_listener() {
        let file = || Some("127.0.0.1:9100".to_string());
        // Explicitly empty is a value, even over the file's address.
        assert_eq!(metrics_bind(Some(String::new()), file()), "");
        assert_eq!(metrics_bind(None, file()), "127.0.0.1:9100");
        assert_eq!(metrics_bind(None, None), "0.0.0.0:9095");
        assert_eq!(
            metrics_bind(Some("[::1]:9095".into()), file()),
            "[::1]:9095"
        );
    }
}
