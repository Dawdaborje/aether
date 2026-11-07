//! Keep working when the main storage cannot be reached.
//!
//! [`FallbackBackend`] wraps the configured backend (S3, say) and a local one. A `put` that the main
//! backend cannot take because it is unreachable is written to the local backend instead, and the key
//! is noted in a [`PendingJournal`]. Reads look in the main backend first and then in the local one,
//! so a file is readable the moment it is stored, wherever it is. [`FallbackBackend::drain`] moves
//! the pending files to the main backend when it answers again, removes the local copies and
//! forgets the notes. A key never changes (it names the file, not the place), so nothing that
//! refers to the file has to be rewritten.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use bytes::Bytes;

use crate::{MediaBackend, MediaKey, MediaObject, StorageError};

/// A file stored locally that still has to reach the main backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingEntry {
    pub key: String,
    pub size: u64,
    /// Uploads tried so far.
    pub attempts: u32,
}

/// Where the files waiting for the main backend are written down. The kernel keeps it in its
/// database so it survives a restart.
#[async_trait]
pub trait PendingJournal: Send + Sync {
    /// Note that `key` is held locally (again for the same key is harmless).
    async fn record(&self, key: &str, size: u64, reason: &str) -> Result<(), StorageError>;
    /// The key reached the main backend, or was deleted.
    async fn forget(&self, key: &str) -> Result<(), StorageError>;
    /// Every file still waiting, oldest first.
    async fn pending(&self) -> Result<Vec<PendingEntry>, StorageError>;
    /// An upload of `key` failed with `reason`.
    async fn failed(&self, key: &str, reason: &str) -> Result<(), StorageError>;
}

/// What one [`FallbackBackend::drain`] did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DrainReport {
    pub uploaded: usize,
    /// Files still waiting after this pass.
    pub remaining: usize,
}

/// Main backend with a local one behind it. See the module documentation.
pub struct FallbackBackend {
    primary: Arc<dyn MediaBackend>,
    local: Arc<dyn MediaBackend>,
    journal: Arc<dyn PendingJournal>,
    /// How long the main backend gets to answer a write before the local copy is used.
    write_timeout: Duration,
}

/// Is trying the same call later likely to work? A missing object, a bad key or a wrong
/// configuration will not get better by waiting, and must not be hidden by the fallback.
fn is_outage(error: &StorageError) -> bool {
    matches!(error, StorageError::Backend(_))
}

impl FallbackBackend {
    pub fn new(
        primary: Arc<dyn MediaBackend>,
        local: Arc<dyn MediaBackend>,
        journal: Arc<dyn PendingJournal>,
        write_timeout: Duration,
    ) -> Self {
        Self { primary, local, journal, write_timeout }
    }

    async fn primary_put(&self, key: &MediaKey, data: Bytes) -> Result<MediaObject, StorageError> {
        match tokio::time::timeout(self.write_timeout, self.primary.put(key, data)).await {
            Ok(result) => result,
            Err(_) => Err(StorageError::Backend("the storage did not answer in time".into())),
        }
    }

    /// Upload every pending file to the main backend. Stops at the first outage: the rest would
    /// fail the same way.
    pub async fn drain(&self) -> Result<DrainReport, StorageError> {
        let pending = self.journal.pending().await?;
        let mut report = DrainReport::default();
        let total = pending.len();
        for entry in pending {
            let Ok(key) = MediaKey::parse(&entry.key) else {
                self.journal.forget(&entry.key).await?;
                continue;
            };
            let data = match self.local.get(&key).await {
                Ok(data) => data,
                Err(StorageError::NotFound(_)) => {
                    // Deleted or already moved: nothing is waiting any more.
                    self.journal.forget(&entry.key).await?;
                    continue;
                }
                Err(error) => {
                    self.journal.failed(&entry.key, &error.to_string()).await?;
                    continue;
                }
            };
            match self.primary_put(&key, data).await {
                Ok(_) => {
                    self.local.delete(&key).await?;
                    self.journal.forget(&entry.key).await?;
                    report.uploaded += 1;
                }
                Err(error) => {
                    self.journal.failed(&entry.key, &error.to_string()).await?;
                    if is_outage(&error) {
                        break;
                    }
                }
            }
        }
        report.remaining = total - report.uploaded;
        Ok(report)
    }
}

#[async_trait]
impl MediaBackend for FallbackBackend {
    async fn put(&self, key: &MediaKey, data: Bytes) -> Result<MediaObject, StorageError> {
        match self.primary_put(key, data.clone()).await {
            Ok(object) => {
                // A newer copy in the main backend supersedes a waiting local one.
                if self.local.exists(key).await.unwrap_or(false) {
                    let _ = self.local.delete(key).await;
                    let _ = self.journal.forget(key.as_str()).await;
                }
                Ok(object)
            }
            Err(error) if is_outage(&error) => {
                log::warn!("media: storage unreachable ({error}); keeping `{key}` locally until it is back");
                let size = data.len() as u64;
                let stored = self.local.put(key, data).await?;
                self.journal.record(key.as_str(), size, &error.to_string()).await?;
                Ok(stored)
            }
            Err(error) => Err(error),
        }
    }

    async fn get(&self, key: &MediaKey) -> Result<Bytes, StorageError> {
        match self.primary.get(key).await {
            Ok(data) => Ok(data),
            Err(error) if is_outage(&error) || matches!(error, StorageError::NotFound(_)) => {
                match self.local.get(key).await {
                    Ok(data) => Ok(data),
                    Err(StorageError::NotFound(_)) => Err(error),
                    Err(other) => Err(other),
                }
            }
            Err(error) => Err(error),
        }
    }

    async fn head(&self, key: &MediaKey) -> Result<MediaObject, StorageError> {
        match self.primary.head(key).await {
            Ok(object) => Ok(object),
            Err(error) if is_outage(&error) || matches!(error, StorageError::NotFound(_)) => {
                match self.local.head(key).await {
                    Ok(object) => Ok(object),
                    Err(StorageError::NotFound(_)) => Err(error),
                    Err(other) => Err(other),
                }
            }
            Err(error) => Err(error),
        }
    }

    async fn delete(&self, key: &MediaKey) -> Result<(), StorageError> {
        let held_locally = self.local.exists(key).await.unwrap_or(false);
        if held_locally {
            self.local.delete(key).await?;
            self.journal.forget(key.as_str()).await?;
        }
        match self.primary.delete(key).await {
            Ok(()) => Ok(()),
            // The file only ever lived here: it is gone, which is what was asked.
            Err(error) if held_locally && is_outage(&error) => Ok(()),
            Err(error) => Err(error),
        }
    }

    async fn list(&self, prefix: Option<&MediaKey>) -> Result<Vec<MediaObject>, StorageError> {
        let mut objects = match self.primary.list(prefix).await {
            Ok(objects) => objects,
            Err(error) if is_outage(&error) => Vec::new(),
            Err(error) => return Err(error),
        };
        for object in self.local.list(prefix).await? {
            if !objects.iter().any(|known| known.key == object.key) {
                objects.push(object);
            }
        }
        Ok(objects)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    };

    use object_store::memory::InMemory;

    use super::*;
    use crate::ObjectStoreBackend;

    /// A backend that can be switched off, as a network cut would.
    struct Switch {
        inner: ObjectStoreBackend,
        down: Arc<AtomicBool>,
    }

    impl Switch {
        fn check(&self) -> Result<(), StorageError> {
            if self.down.load(Ordering::SeqCst) {
                Err(StorageError::Backend("connection refused".into()))
            } else {
                Ok(())
            }
        }
    }

    #[async_trait]
    impl MediaBackend for Switch {
        async fn put(&self, key: &MediaKey, data: Bytes) -> Result<MediaObject, StorageError> {
            self.check()?;
            self.inner.put(key, data).await
        }
        async fn get(&self, key: &MediaKey) -> Result<Bytes, StorageError> {
            self.check()?;
            self.inner.get(key).await
        }
        async fn head(&self, key: &MediaKey) -> Result<MediaObject, StorageError> {
            self.check()?;
            self.inner.head(key).await
        }
        async fn delete(&self, key: &MediaKey) -> Result<(), StorageError> {
            self.check()?;
            self.inner.delete(key).await
        }
        async fn list(&self, prefix: Option<&MediaKey>) -> Result<Vec<MediaObject>, StorageError> {
            self.check()?;
            self.inner.list(prefix).await
        }
    }

    #[derive(Default)]
    struct Memory(Mutex<Vec<PendingEntry>>);

    #[async_trait]
    impl PendingJournal for Memory {
        async fn record(&self, key: &str, size: u64, _reason: &str) -> Result<(), StorageError> {
            let mut all = self.0.lock().unwrap_or_else(|e| e.into_inner());
            if !all.iter().any(|entry| entry.key == key) {
                all.push(PendingEntry { key: key.to_string(), size, attempts: 0 });
            }
            Ok(())
        }
        async fn forget(&self, key: &str) -> Result<(), StorageError> {
            self.0.lock().unwrap_or_else(|e| e.into_inner()).retain(|entry| entry.key != key);
            Ok(())
        }
        async fn pending(&self) -> Result<Vec<PendingEntry>, StorageError> {
            Ok(self.0.lock().unwrap_or_else(|e| e.into_inner()).clone())
        }
        async fn failed(&self, key: &str, _reason: &str) -> Result<(), StorageError> {
            for entry in self.0.lock().unwrap_or_else(|e| e.into_inner()).iter_mut() {
                if entry.key == key {
                    entry.attempts += 1;
                }
            }
            Ok(())
        }
    }

    fn setup() -> (FallbackBackend, Arc<AtomicBool>, Arc<Memory>, Arc<dyn MediaBackend>) {
        let down = Arc::new(AtomicBool::new(false));
        let primary: Arc<dyn MediaBackend> =
            Arc::new(Switch { inner: ObjectStoreBackend::new(InMemory::new()), down: down.clone() });
        let local: Arc<dyn MediaBackend> = Arc::new(ObjectStoreBackend::new(InMemory::new()));
        let journal = Arc::new(Memory::default());
        let backend = FallbackBackend::new(primary.clone(), local, journal.clone(), Duration::from_secs(5));
        (backend, down, journal, primary)
    }

    #[tokio::test]
    async fn an_outage_keeps_the_file_locally_and_a_drain_moves_it() -> Result<(), Box<dyn std::error::Error>> {
        let (backend, down, journal, primary) = setup();
        let key = MediaKey::parse("orgs/acme/a.txt")?;

        down.store(true, Ordering::SeqCst);
        backend.put(&key, Bytes::from_static(b"hello")).await?;
        assert_eq!(backend.get(&key).await?, Bytes::from_static(b"hello"));
        assert_eq!(journal.pending().await?.len(), 1);
        // Still down: the drain keeps the file and counts the attempt.
        let report = backend.drain().await?;
        assert_eq!((report.uploaded, report.remaining), (0, 1));
        assert_eq!(journal.pending().await?[0].attempts, 1);

        down.store(false, Ordering::SeqCst);
        let report = backend.drain().await?;
        assert_eq!((report.uploaded, report.remaining), (1, 0));
        assert!(journal.pending().await?.is_empty());
        assert_eq!(primary.get(&key).await?, Bytes::from_static(b"hello"));
        assert_eq!(backend.get(&key).await?, Bytes::from_static(b"hello"));
        Ok(())
    }

    #[tokio::test]
    async fn a_healthy_backend_never_uses_the_local_one() -> Result<(), Box<dyn std::error::Error>> {
        let (backend, _down, journal, primary) = setup();
        let key = MediaKey::parse("a.txt")?;
        backend.put(&key, Bytes::from_static(b"x")).await?;
        assert!(journal.pending().await?.is_empty());
        assert!(primary.exists(&key).await?);
        Ok(())
    }

    #[tokio::test]
    async fn deleting_a_waiting_file_forgets_it() -> Result<(), Box<dyn std::error::Error>> {
        let (backend, down, journal, _primary) = setup();
        let key = MediaKey::parse("a.txt")?;
        down.store(true, Ordering::SeqCst);
        backend.put(&key, Bytes::from_static(b"x")).await?;
        backend.delete(&key).await?;
        assert!(journal.pending().await?.is_empty());
        assert!(matches!(backend.get(&key).await, Err(StorageError::Backend(_))));
        Ok(())
    }

    #[tokio::test]
    async fn a_missing_file_is_not_found_not_an_outage() -> Result<(), Box<dyn std::error::Error>> {
        let (backend, _down, _journal, _primary) = setup();
        let key = MediaKey::parse("nope.txt")?;
        assert!(matches!(backend.get(&key).await, Err(StorageError::NotFound(_))));
        Ok(())
    }
}
