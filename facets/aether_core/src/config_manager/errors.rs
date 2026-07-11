use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file '{path}': {source}")]
    Read { path: String, source: io::Error },

    #[error("failed to parse config file: {0}")]
    Parse(#[from] toml::de::Error),

    #[error("missing required config field '{0}'")]
    MissingField(String),

    #[error("config field '{field}' must be of type {expected}")]
    InvalidType { field: String, expected: String },
}
