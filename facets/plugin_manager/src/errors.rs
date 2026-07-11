use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("no plugin.toml or workspace.toml found at '{path}'")]
    MissingManifest { path: String },

    #[error("failed to read manifest '{path}': {source}")]
    Read { path: String, source: io::Error },

    #[error("failed to parse manifest '{path}': {source}")]
    Parse {
        path: String,
        source: toml::de::Error,
    },
}
