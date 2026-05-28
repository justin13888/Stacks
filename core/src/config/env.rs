use std::env;

use super::{
    ConfigError,
    file::FileConfig,
    models::{
        AuthConfig, BeamConfig, CameoConfig, Config, DatabaseConfig, OidcConfig, ServerConfig,
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

/// Parse a comma-separated origin list from the env, falling back to the file
/// value, then to an empty list (which the API treats as the localhost
/// developer default).
fn parse_origins(env_value: Option<&str>, file_value: Option<Vec<String>>) -> Vec<String> {
    if let Some(raw) = env_value {
        return raw
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    file_value.unwrap_or_default()
}

/// Build the configuration, layering environment variables over `config.toml`.
pub fn build(file: FileConfig) -> Result<Config, ConfigError> {
    let data_dir = optional("WW_DATA_DIR", file.server.data_dir, "/data");
    Ok(Config {
        database: DatabaseConfig {
            url: require("WW_DATABASE_URL", file.database.url)?,
        },
        auth: AuthConfig {
            secret_key: require("WW_SECRET_KEY", file.auth.secret_key)?,
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
        },
        server: ServerConfig {
            bind: optional("WW_BIND", file.server.bind, "0.0.0.0:8095"),
            log_level: optional("WW_LOG_LEVEL", file.server.log_level, "info"),
            data_dir: data_dir.clone(),
            cors_allowed_origins: parse_origins(
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
        beam: BeamConfig {
            sync_interval_secs: optional_u64(
                "WW_BEAM_SYNC_INTERVAL_SECS",
                file.beam.sync_interval_secs,
                3600,
            ),
        },
        oidc: OidcConfig {
            issuer: require("WW_OIDC_ISSUER", file.oidc.issuer)?,
            client_id: require("WW_OIDC_CLIENT_ID", file.oidc.client_id)?,
            client_secret: require("WW_OIDC_CLIENT_SECRET", file.oidc.client_secret)?,
            redirect_uri: require("WW_OIDC_REDIRECT_URI", file.oidc.redirect_uri)?,
        },
    })
}
