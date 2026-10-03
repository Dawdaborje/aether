//! The storage an organization owns, created when the organization is.
//!
//! * `<app_dir>/orgs/<organization>/` for its files (with a `conf/` folder).
//! * `orgs/<organization>/` in the media backend: a directory under the local
//!   media directory, or a key prefix in the configured S3 bucket. One bucket
//!   shared by all organizations with a prefix each is the usual S3 layout: it
//!   needs no permission to create buckets and has no per-account bucket limit,
//!   and a [`PrefixedBackend`] keeps each organization inside its own prefix.
//!
//! A small marker object is written through the backend, so the folder exists
//! (object stores have no empty folders) and a misconfigured or read-only
//! backend fails when the organization is created rather than at first upload.

use std::{path::PathBuf, sync::Arc};

use aether_storage::{MediaBackend, MediaKey, PrefixedBackend, StorageError, org_media_prefix};
use bytes::Bytes;
use thiserror::Error;

use crate::app_dir::{AppDir, AppDirError};

/// Name of the marker object inside an organization's media prefix.
pub const ORG_MARKER: &str = ".aether-org";

#[derive(Debug, Error)]
pub enum OrgStorageError {
    #[error(transparent)]
    AppDir(#[from] AppDirError),

    #[error("organization media storage: {0}")]
    Media(#[from] StorageError),
}

/// What [`provision_org_storage`] created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrgStorage {
    /// `<app_dir>/orgs/<organization>`
    pub directory: PathBuf,
    /// `orgs/<organization>` in the media backend.
    pub media_prefix: String,
}

/// Create an organization's folder in `app_dir` and its place in the media
/// backend. Safe to repeat.
pub async fn provision_org_storage(
    app_dir: &AppDir,
    media: Arc<dyn MediaBackend>,
    organization: &str,
) -> Result<OrgStorage, OrgStorageError> {
    let directory = app_dir.ensure_org(organization).await?;

    let media_prefix = org_media_prefix(organization)?;
    let scoped = PrefixedBackend::new(media, &media_prefix)?;
    scoped
        .put(
            &MediaKey::parse(ORG_MARKER)?,
            Bytes::from(format!("{{\"organization\":\"{organization}\"}}")),
        )
        .await?;

    Ok(OrgStorage {
        directory,
        media_prefix,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_storage::ObjectStoreBackend;
    use object_store::memory::InMemory;

    #[tokio::test]
    async fn creates_the_folder_and_the_media_prefix() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let app_dir = AppDir::new(directory.path().join("app"));
        let media: Arc<dyn MediaBackend> = Arc::new(ObjectStoreBackend::new(InMemory::new()));

        let storage = provision_org_storage(&app_dir, media.clone(), "acme").await?;
        assert_eq!(storage.media_prefix, "orgs/acme");
        assert!(storage.directory.join("conf").is_dir());

        let marker = media.get(&MediaKey::parse("orgs/acme/.aether-org")?).await?;
        assert_eq!(marker, Bytes::from_static(b"{\"organization\":\"acme\"}"));

        // Repeating is harmless; a bad name is refused before anything is written.
        provision_org_storage(&app_dir, media.clone(), "acme").await?;
        assert!(provision_org_storage(&app_dir, media.clone(), "../evil").await.is_err());
        assert!(media.list(None).await?.iter().all(|o| o.key.starts_with("orgs/acme/")));
        Ok(())
    }
}
