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

/// Optional u64: environment variable, else config file, else default.
fn optional_u64(key: &str, file: Option<u64>, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .or(file)
        .unwrap_or(default)
}

/// Parse a comma-separated list from the env, falling back to the file value,
/// then to an empty list. Used for both the CORS allowlist (where empty means
/// "localhost developer defaults") and the admin-email allowlist.
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

/// Reject secrets too weak to sign JWTs.
///
/// The 32-byte minimum was documented but never enforced, and the shipped
/// `.env.example` placeholder would silently start a real server.
fn validate_secret_key(key: &str) -> Result<(), ConfigError> {
    const PLACEHOLDERS: [&str; 4] = ["changeme", "change-me", "secret", "your-secret-key"];
    if PLACEHOLDERS.contains(&key.to_ascii_lowercase().trim()) {
        return Err(ConfigError::Invalid(
            "WW_SECRET_KEY is still the example placeholder; generate one with `just gen-secret`"
                .to_string(),
        ));
    }
    if key.len() < 32 {
        return Err(ConfigError::Invalid(format!(
            "WW_SECRET_KEY must be at least 32 bytes (got {}); generate one with `just gen-secret`",
            key.len()
        )));
    }
    Ok(())
}

/// Build the configuration, layering environment variables over `config.toml`.
pub fn build(file: FileConfig) -> Result<Config, ConfigError> {
    let data_dir = optional("WW_DATA_DIR", file.server.data_dir, "/data");
    let secret_key = require("WW_SECRET_KEY", file.auth.secret_key)?;
    validate_secret_key(&secret_key)?;
    Ok(Config {
        database: DatabaseConfig {
            url: require("WW_DATABASE_URL", file.database.url)?,
        },
        auth: AuthConfig {
            secret_key,
            access_token_ttl_secs: optional_u64(
                "WW_ACCESS_TOKEN_TTL_SECS",
                file.auth.access_token_ttl_secs,
                900,
            ),
            refresh_token_ttl_secs: optional_u64(
                "WW_REFRESH_TOKEN_TTL_SECS",
                file.auth.refresh_token_ttl_secs,
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
            cors_allowed_origins: parse_csv(
                env::var("WW_CORS_ALLOWED_ORIGINS").ok().as_deref(),
                file.server.cors_allowed_origins,
            ),
        },
        cameo: CameoConfig {
            tmdb_api_key: require("WW_TMDB_API_KEY", file.cameo.tmdb_api_key)?,
            cache_dir: optional(
                "WW_CAMEO_CACHE_DIR",
                file.cameo.cache_dir,
                &format!("{data_dir}/cameo"),
            ),
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
            internal_issuer: env::var("WW_OIDC_INTERNAL_ISSUER")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .or(file.oidc.internal_issuer),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_key_must_be_long_enough() {
        assert!(validate_secret_key(&"a".repeat(32)).is_ok());
        assert!(validate_secret_key(&"a".repeat(64)).is_ok());
        let err = validate_secret_key(&"a".repeat(31)).unwrap_err();
        assert!(err.to_string().contains("at least 32 bytes"));
    }

    #[test]
    fn example_placeholders_are_rejected() {
        // The shipped .env.example used to start a real server as-is.
        for placeholder in ["changeme", "CHANGEME", " change-me ", "secret"] {
            assert!(
                validate_secret_key(placeholder).is_err(),
                "{placeholder:?} should be rejected"
            );
        }
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
