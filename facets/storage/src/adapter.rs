use async_trait::async_trait;
use bytes::Bytes;
use futures::TryStreamExt;
use object_store::{ObjectStore, ObjectStoreExt, PutPayload, path::Path};

use crate::{MediaBackend, MediaKey, MediaObject, StorageError};

/// [`MediaBackend`] over any [`ObjectStore`] (local disk, S3, Garage, MinIO, …).
/// Integration crates only have to build the store.
#[derive(Debug)]
pub struct ObjectStoreBackend {
    store: std::sync::Arc<dyn ObjectStore>,
}

impl ObjectStoreBackend {
    pub fn new(store: impl ObjectStore) -> Self {
        Self {
            store: std::sync::Arc::new(store),
        }
    }
}

fn path_for(key: &MediaKey) -> Result<Path, StorageError> {
    Path::parse(key.as_str()).map_err(|_| StorageError::InvalidKey {
        key: key.to_string(),
        reason: "key is not representable by the storage backend",
    })
}

fn map_error(key: &str, error: object_store::Error) -> StorageError {
    match error {
        object_store::Error::NotFound { .. } => StorageError::NotFound(key.to_string()),
        other => StorageError::Backend(Box::new(other)),
    }
}

#[async_trait]
impl MediaBackend for ObjectStoreBackend {
    async fn put(&self, key: &MediaKey, data: Bytes) -> Result<MediaObject, StorageError> {
        let size = data.len() as u64;
        let result = self
            .store
            .put(&path_for(key)?, PutPayload::from_bytes(data))
            .await
            .map_err(|error| map_error(key.as_str(), error))?;
        Ok(MediaObject {
            key: key.to_string(),
            size,
            e_tag: result.e_tag,
        })
    }

    async fn get(&self, key: &MediaKey) -> Result<Bytes, StorageError> {
        self.store
            .get(&path_for(key)?)
            .await
            .map_err(|error| map_error(key.as_str(), error))?
            .bytes()
            .await
            .map_err(|error| map_error(key.as_str(), error))
    }

    async fn head(&self, key: &MediaKey) -> Result<MediaObject, StorageError> {
        let meta = self
            .store
            .head(&path_for(key)?)
            .await
            .map_err(|error| map_error(key.as_str(), error))?;
        Ok(MediaObject {
            key: key.to_string(),
            size: meta.size,
            e_tag: meta.e_tag,
        })
    }

    async fn delete(&self, key: &MediaKey) -> Result<(), StorageError> {
        match self.store.delete(&path_for(key)?).await {
            Ok(()) | Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(error) => Err(StorageError::Backend(Box::new(error))),
        }
    }

    async fn list(&self, prefix: Option<&MediaKey>) -> Result<Vec<MediaObject>, StorageError> {
        let prefix = prefix.map(path_for).transpose()?;
        self.store
            .list(prefix.as_ref())
            .map_ok(|meta| MediaObject {
                key: meta.location.to_string(),
                size: meta.size,
                e_tag: meta.e_tag,
            })
            .try_collect()
            .await
            .map_err(|error| StorageError::Backend(Box::new(error)))
    }
}
