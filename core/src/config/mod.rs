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

/// Load config: env vars take precedence over config.toml.
pub fn load() -> Result<Config, ConfigError> {
    env::from_env()
}
