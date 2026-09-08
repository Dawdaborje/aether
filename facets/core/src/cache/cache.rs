use std::sync::Arc;
use std::time::Duration;

use super::config::{CacheBackendKind, CacheConfig};
use super::error::CacheError;
use super::moka::MokaCache;
use super::redis_store::RedisCache;
use super::store::{CacheStore, CacheValue, namespaced_key};

/// Kernel-facing cache facade. All plugin access should go through namespaced methods so
/// org/plugin isolation is enforced by the kernel rather than by callers.
#[derive(Clone)]
pub struct Cache {
    store: Arc<dyn CacheStore>,
    default_ttl: Option<Duration>,
    max_value_bytes: usize,
    backend: CacheBackendKind,
}

impl Cache {
    pub fn new(store: Arc<dyn CacheStore>, config: &CacheConfig) -> Self {
        Self {
            store,
            default_ttl: config.default_ttl_secs.map(Duration::from_secs),
            max_value_bytes: config.max_value_bytes,
            backend: config.backend,
        }
    }

    /// Build a cache from config (`moka` by default, or `redis` when configured).
    pub fn from_config(config: &CacheConfig) -> Result<Self, CacheError> {
        let store: Arc<dyn CacheStore> = match config.backend {
            CacheBackendKind::Moka => Arc::new(MokaCache::new(config)),
            CacheBackendKind::Redis => {
                let redis = config
                    .redis
                    .as_ref()
                    .ok_or(CacheError::MissingBackendConfig {
                        backend: "redis",
                        section: "cache.redis",
                    })?;
                Arc::new(RedisCache::connect(redis, config.max_value_bytes)?)
            }
        };
        Ok(Self::new(store, config))
    }

    pub fn backend(&self) -> CacheBackendKind {
        self.backend
    }

    pub fn max_value_bytes(&self) -> usize {
        self.max_value_bytes
    }

    pub fn len(&self) -> usize {
        self.store.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, namespace: &str, key: &str) -> Result<Option<CacheValue>, CacheError> {
        let fq = namespaced_key(namespace, key)?;
        self.store.get(&fq)
    }

    pub fn set(
        &self,
        namespace: &str,
        key: &str,
        value: impl Into<CacheValue>,
        ttl: Option<Duration>,
    ) -> Result<(), CacheError> {
        let fq = namespaced_key(namespace, key)?;
        let value = value.into();
        let ttl = ttl.or(self.default_ttl);
        self.store.set(&fq, value, ttl)
    }

    pub fn invalidate(&self, namespace: &str, key: &str) -> Result<bool, CacheError> {
        let fq = namespaced_key(namespace, key)?;
        self.store.delete(&fq)
    }

    /// Invalidate every key under `namespace` whose key starts with `prefix`.
    pub fn invalidate_prefix(&self, namespace: &str, prefix: &str) -> Result<usize, CacheError> {
        if namespace.is_empty() {
            return Err(CacheError::EmptyNamespace);
        }
        let fq_prefix = format!("{namespace}::{prefix}");
        self.store.delete_prefix(&fq_prefix)
    }

    /// Drop the entire namespace (e.g. active org).
    pub fn clear_namespace(&self, namespace: &str) -> Result<usize, CacheError> {
        if namespace.is_empty() {
            return Err(CacheError::EmptyNamespace);
        }
        let prefix = format!("{namespace}::");
        self.store.delete_prefix(&prefix)
    }
}
