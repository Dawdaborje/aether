use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CacheError {
    #[error("cache key is empty")]
    EmptyKey,

    #[error("cache namespace is empty")]
    EmptyNamespace,

    #[error("cache value exceeds max size ({max} bytes)")]
    ValueTooLarge { max: usize },

    #[error("cache is full and could not evict an entry")]
    Full,

    #[error("unsupported cache backend `{0}`")]
    UnsupportedBackend(String),

    #[error("cache backend `{backend}` requires [{section}] in aether.toml")]
    MissingBackendConfig {
        backend: &'static str,
        section: &'static str,
    },

    #[error("cache backend error: {0}")]
    Backend(String),
}
