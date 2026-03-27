use std::env;

use super::{
    ConfigError,
    models::{
        AuthConfig, BeamConfig, CameoConfig, Config, DatabaseConfig, OidcConfig, ServerConfig,
    },
};

fn require(key: &'static str) -> Result<String, ConfigError> {
    env::var(key).map_err(|_| ConfigError::MissingEnvVar(key))
}

fn optional(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

fn optional_u64(key: &str, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

pub fn from_env() -> Result<Config, ConfigError> {
    let data_dir = optional("WW_DATA_DIR", "/data");
    Ok(Config {
        database: DatabaseConfig {
            url: require("WW_DATABASE_URL")?,
        },
        auth: AuthConfig {
            secret_key: require("WW_SECRET_KEY")?,
            access_token_ttl_secs: optional_u64("WW_ACCESS_TOKEN_TTL_SECS", 900),
            refresh_token_ttl_secs: optional_u64("WW_REFRESH_TOKEN_TTL_SECS", 2_592_000),
        },
        server: ServerConfig {
            bind: optional("WW_BIND", "0.0.0.0:8095"),
            log_level: optional("WW_LOG_LEVEL", "info"),
            data_dir: data_dir.clone(),
        },
        cameo: CameoConfig {
            tmdb_api_key: require("WW_TMDB_API_KEY")?,
            cache_dir: optional("WW_CAMEO_CACHE_DIR", &format!("{data_dir}/cameo")),
        },
        beam: BeamConfig {
            sync_interval_secs: optional_u64("WW_BEAM_SYNC_INTERVAL_SECS", 3600),
        },
        oidc: OidcConfig {
            issuer: require("WW_OIDC_ISSUER")?,
            client_id: require("WW_OIDC_CLIENT_ID")?,
            client_secret: require("WW_OIDC_CLIENT_SECRET")?,
            redirect_uri: require("WW_OIDC_REDIRECT_URI")?,
        },
    })
}
