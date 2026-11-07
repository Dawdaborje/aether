//! Local-disk media storage bridge.

pub mod model;

use aether_storage::{ObjectStoreBackend, StorageError};
use object_store::local::LocalFileSystem;

pub use model::LocalStorageConfig;

/// Create the base directory if needed and open it as a media backend.
pub async fn build(config: &LocalStorageConfig) -> Result<ObjectStoreBackend, StorageError> {
    tokio::fs::create_dir_all(&config.base_path)
        .await
        .map_err(|error| {
            StorageError::Config(format!(
                "cannot create media directory {}: {error}",
                config.base_path.display()
            ))
        })?;
    let store = LocalFileSystem::new_with_prefix(&config.base_path)
        .map_err(|error| StorageError::Backend(Box::new(error)))?;
    Ok(ObjectStoreBackend::new(store))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_storage::{MediaBackend, MediaKey};
    use bytes::Bytes;

    #[tokio::test]
    async fn stores_reads_lists_and_deletes() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let backend = build(&LocalStorageConfig {
            base_path: directory.path().join("media"),
        })
        .await?;
        let key = MediaKey::parse("invoices/2026/a.txt")?;

        assert!(!backend.exists(&key).await?);
        let stored = backend.put(&key, Bytes::from_static(b"hello")).await?;
        assert_eq!(stored.size, 5);
        assert_eq!(backend.get(&key).await?, Bytes::from_static(b"hello"));
        assert_eq!(backend.head(&key).await?.size, 5);

        let listed = backend.list(Some(&MediaKey::parse("invoices")?)).await?;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].key, "invoices/2026/a.txt");

        backend.delete(&key).await?;
        backend.delete(&key).await?;
        assert!(!backend.exists(&key).await?);
        assert!(matches!(
            backend.get(&key).await,
            Err(StorageError::NotFound(_))
        ));
        Ok(())
    }
}
