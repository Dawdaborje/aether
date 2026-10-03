use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("invalid media key `{key}`: {reason}")]
    InvalidKey { key: String, reason: &'static str },

    #[error("media object `{0}` was not found")]
    NotFound(String),

    #[error("storage backend is misconfigured: {0}")]
    Config(String),

    #[error("storage backend error: {0}")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),
}
