use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;

use crate::{MediaBackend, MediaKey, MediaObject, StorageError};

/// Where an organization's media lives inside the shared backend:
/// `orgs/<organization>/`. On local storage this is a directory; on S3 it is a
/// key prefix in the configured bucket. The organization name is a database
/// name, so only letters, digits and `_` are accepted.
pub fn org_media_prefix(organization: &str) -> Result<String, StorageError> {
    let valid = !organization.is_empty()
        && organization
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !valid {
        return Err(StorageError::InvalidKey {
            key: organization.to_string(),
            reason: "an organization name is letters, digits and `_`",
        });
    }
    Ok(format!("orgs/{organization}"))
}

/// A view of another backend in which every key is stored under a prefix.
/// A plugin handed one of these for its organization cannot name, list or read
/// anything outside it.
#[derive(Clone)]
pub struct PrefixedBackend {
    inner: Arc<dyn MediaBackend>,
    prefix: MediaKey,
}

impl PrefixedBackend {
    pub fn new(inner: Arc<dyn MediaBackend>, prefix: &str) -> Result<Self, StorageError> {
        Ok(Self {
            inner,
            prefix: MediaKey::parse(prefix)?,
        })
    }

    /// The backend for one organization.
    pub fn for_organization(
        inner: Arc<dyn MediaBackend>,
        organization: &str,
    ) -> Result<Self, StorageError> {
        Self::new(inner, &org_media_prefix(organization)?)
    }

    fn full(&self, key: &MediaKey) -> Result<MediaKey, StorageError> {
        MediaKey::parse(&format!("{}/{key}", self.prefix))
    }

    fn strip(&self, mut object: MediaObject) -> MediaObject {
        if let Some(rest) = object.key.strip_prefix(&format!("{}/", self.prefix)) {
            object.key = rest.to_string();
        }
        object
    }
}

#[async_trait]
impl MediaBackend for PrefixedBackend {
    async fn put(&self, key: &MediaKey, data: Bytes) -> Result<MediaObject, StorageError> {
        Ok(self.strip(self.inner.put(&self.full(key)?, data).await?))
    }

    async fn get(&self, key: &MediaKey) -> Result<Bytes, StorageError> {
        self.inner.get(&self.full(key)?).await
    }

    async fn head(&self, key: &MediaKey) -> Result<MediaObject, StorageError> {
        Ok(self.strip(self.inner.head(&self.full(key)?).await?))
    }

    async fn delete(&self, key: &MediaKey) -> Result<(), StorageError> {
        self.inner.delete(&self.full(key)?).await
    }

    async fn list(&self, prefix: Option<&MediaKey>) -> Result<Vec<MediaObject>, StorageError> {
        let scope = match prefix {
            Some(prefix) => self.full(prefix)?,
            None => self.prefix.clone(),
        };
        Ok(self
            .inner
            .list(Some(&scope))
            .await?
            .into_iter()
            .map(|object| self.strip(object))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ObjectStoreBackend;
    use object_store::memory::InMemory;

    fn shared() -> Arc<dyn MediaBackend> {
        Arc::new(ObjectStoreBackend::new(InMemory::new()))
    }

    #[test]
    fn organization_prefixes_are_safe() -> Result<(), StorageError> {
        assert_eq!(org_media_prefix("acme_2")?, "orgs/acme_2");
        for bad in ["", "../x", "a/b", "a b", "a-b", "a.b"] {
            assert!(org_media_prefix(bad).is_err(), "{bad:?}");
        }
        Ok(())
    }

    #[tokio::test]
    async fn keys_are_scoped_and_organizations_are_isolated() -> Result<(), Box<dyn std::error::Error>> {
        let backend = shared();
        let acme = PrefixedBackend::for_organization(backend.clone(), "acme")?;
        let globex = PrefixedBackend::for_organization(backend.clone(), "globex")?;
        let key = MediaKey::parse("invoices/a.txt")?;

        let stored = acme.put(&key, Bytes::from_static(b"acme data")).await?;
        assert_eq!(stored.key, "invoices/a.txt", "callers never see the prefix");

        // It really lives under the prefix in the shared backend...
        let raw = backend.get(&MediaKey::parse("orgs/acme/invoices/a.txt")?).await?;
        assert_eq!(raw, Bytes::from_static(b"acme data"));
        // ...and the other organization cannot see it.
        assert!(matches!(globex.get(&key).await, Err(StorageError::NotFound(_))));
        assert!(globex.list(None).await?.is_empty());

        let listed = acme.list(None).await?;
        assert_eq!(listed.iter().map(|o| o.key.as_str()).collect::<Vec<_>>(), ["invoices/a.txt"]);
        let nested = acme.list(Some(&MediaKey::parse("invoices")?)).await?;
        assert_eq!(nested.len(), 1);

        acme.delete(&key).await?;
        assert!(!acme.exists(&key).await?);
        Ok(())
    }
}
