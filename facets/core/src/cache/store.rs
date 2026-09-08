use std::time::Duration;

use super::error::CacheError;

/// Opaque cached payload. Bytes keep the facet language-agnostic for future WASM hosts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheValue {
    pub bytes: Vec<u8>,
}

impl CacheValue {
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: bytes.into(),
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

impl From<Vec<u8>> for CacheValue {
    fn from(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }
}

impl From<&[u8]> for CacheValue {
    fn from(bytes: &[u8]) -> Self {
        Self {
            bytes: bytes.to_vec(),
        }
    }
}

impl From<&str> for CacheValue {
    fn from(s: &str) -> Self {
        Self {
            bytes: s.as_bytes().to_vec(),
        }
    }
}

impl From<String> for CacheValue {
    fn from(s: String) -> Self {
        Self {
            bytes: s.into_bytes(),
        }
    }
}

/// Fully qualified cache key: `{namespace}::{key}`.
pub fn namespaced_key(namespace: &str, key: &str) -> Result<String, CacheError> {
    if namespace.is_empty() {
        return Err(CacheError::EmptyNamespace);
    }
    if key.is_empty() {
        return Err(CacheError::EmptyKey);
    }
    Ok(format!("{namespace}::{key}"))
}

/// Low-level store used by [`crate::Cache`]. Backends (Moka, Redis) implement this.
pub trait CacheStore: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<CacheValue>, CacheError>;
    fn set(&self, key: &str, value: CacheValue, ttl: Option<Duration>) -> Result<(), CacheError>;
    fn delete(&self, key: &str) -> Result<bool, CacheError>;
    /// Delete every key that starts with `prefix` (including the prefix itself).
    fn delete_prefix(&self, prefix: &str) -> Result<usize, CacheError>;
    fn clear(&self) -> Result<(), CacheError>;
    fn len(&self) -> usize;
}
