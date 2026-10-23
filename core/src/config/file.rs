//! Optional `config.toml` loading. Environment variables take precedence over
//! any value found in the file.

use std::path::Path;

use serde::Deserialize;

use super::ConfigError;

/// A parsed `config.toml`. Every field is optional: environment variables
/// override anything set here, and any required value left unset is reported
/// by [`build`](super::env::build).
#[derive(Debug, Default, Deserialize)]
pub struct FileConfig {
    #[serde(default)]
    pub database: FileDatabase,
    #[serde(default)]
    pub auth: FileAuth,
    #[serde(default)]
    pub server: FileServer,
    #[serde(default)]
    pub cameo: FileCameo,
    #[serde(default)]
    pub imports: FileImports,
    #[serde(default)]
    pub oidc: FileOidc,
}

#[derive(Debug, Default, Deserialize)]
pub struct FileDatabase {
    pub url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct FileAuth {
    pub secret_key: Option<String>,
    pub access_token_ttl_secs: Option<u64>,
    pub refresh_token_ttl_secs: Option<u64>,
    pub admin_emails: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
pub struct FileServer {
    pub bind: Option<String>,
    pub log_level: Option<String>,
    pub data_dir: Option<String>,
    pub cors_allowed_origins: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
pub struct FileCameo {
    pub tmdb_api_key: Option<String>,
    pub cache_dir: Option<String>,
    pub connect_timeout_secs: Option<u64>,
    pub request_timeout_secs: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct FileImports {
    pub max_file_bytes: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct FileOidc {
    pub issuer: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub redirect_uri: Option<String>,
    pub internal_issuer: Option<String>,
}

/// Load and parse a `config.toml`. Returns `Ok(None)` when the file is absent
/// — a fully environment-variable configuration is valid.
pub fn from_file(path: &Path) -> Result<Option<FileConfig>, ConfigError> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path)?;
    let parsed = toml::from_str(&raw)
        .map_err(|e| ConfigError::Invalid(format!("config file {}: {e}", path.display())))?;
    Ok(Some(parsed))
}
