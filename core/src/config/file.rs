//! Optional config.toml file loading. Environment variables take precedence.
//! This module is reserved for future use.

use super::models::Config;

/// Parse a TOML config file. Not yet implemented.
pub fn from_file(_path: &str) -> Option<Config> {
    // TODO: implement TOML parsing with `toml` crate
    None
}
