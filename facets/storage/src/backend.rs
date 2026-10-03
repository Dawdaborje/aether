use async_trait::async_trait;
use bytes::Bytes;

use crate::{MediaKey, StorageError};

/// Metadata about a stored object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaObject {
    pub key: String,
    pub size: u64,
    pub e_tag: Option<String>,
}

/// Operations the framework needs from a media storage integration.
#[async_trait]
pub trait MediaBackend: Send + Sync {
    /// Store `data` under `key`, replacing any existing object.
    async fn put(&self, key: &MediaKey, data: Bytes) -> Result<MediaObject, StorageError>;

    /// Fetch the full contents of `key`; [`StorageError::NotFound`] if absent.
    async fn get(&self, key: &MediaKey) -> Result<Bytes, StorageError>;

    /// Metadata for `key`; [`StorageError::NotFound`] if absent.
    async fn head(&self, key: &MediaKey) -> Result<MediaObject, StorageError>;

    /// Remove `key`. Deleting a missing object succeeds.
    async fn delete(&self, key: &MediaKey) -> Result<(), StorageError>;

    /// Every object whose key sits under `prefix` (all objects when `None`).
    async fn list(&self, prefix: Option<&MediaKey>) -> Result<Vec<MediaObject>, StorageError>;

    async fn exists(&self, key: &MediaKey) -> Result<bool, StorageError> {
        match self.head(key).await {
            Ok(_) => Ok(true),
            Err(StorageError::NotFound(_)) => Ok(false),
            Err(error) => Err(error),
        }
    }
}
