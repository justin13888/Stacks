pub mod env;
pub mod file;
pub mod models;

pub use models::Config;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required environment variable: {0}")]
    MissingEnvVar(&'static str),
    #[error("invalid configuration: {0}")]
    Invalid(String),
    #[error("failed to read config file: {0}")]
    FileError(#[from] std::io::Error),
}

/// Load configuration: environment variables take precedence over `config.toml`.
///
/// The config file path is taken from `WW_CONFIG_FILE`, defaulting to
/// `config.toml` in the working directory; an absent file is not an error.
pub fn load() -> Result<Config, ConfigError> {
    let path = std::env::var("WW_CONFIG_FILE").unwrap_or_else(|_| "config.toml".to_string());
    let file = file::from_file(std::path::Path::new(&path))?.unwrap_or_default();
    env::build(file)
}
