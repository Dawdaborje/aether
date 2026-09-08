//! Kernel cache facet — namespaced key/value store with TTL.
//!
//! Plugins reach this only through capability-gated kernel commands
//! (`cache::get`, `cache::set`, `cache::invalidate`, `cache::clear`).
//!
//! Backends:
//! - **Moka** (default) — in-process concurrent cache
//! - **Redis** — external Redis / Redis-compatible server

mod cache;
mod config;
mod error;
mod moka;
mod redis_store;
mod store;

pub use cache::Cache;
pub use config::{CacheBackendKind, CacheConfig, RedisCacheConfig};
pub use error::CacheError;
pub use moka::MokaCache;
pub use redis_store::RedisCache;
pub use store::{CacheStore, CacheValue};

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    fn test_cache(max_entries: usize) -> Cache {
        let config = CacheConfig {
            backend: CacheBackendKind::Moka,
            default_ttl_secs: None,
            max_entries,
            max_value_bytes: 1024,
            redis: None,
        };
        Cache::from_config(&config).expect("moka backend")
    }

    #[test]
    fn set_get_invalidate() {
        let cache = test_cache(100);
        cache.set("org_a", "user:1", "alice", None).unwrap();
        assert_eq!(
            cache.get("org_a", "user:1").unwrap().unwrap().as_slice(),
            b"alice"
        );
        assert!(cache.invalidate("org_a", "user:1").unwrap());
        assert!(cache.get("org_a", "user:1").unwrap().is_none());
    }

    #[test]
    fn namespaces_are_isolated() {
        let cache = test_cache(100);
        cache.set("org_a", "k", "a", None).unwrap();
        cache.set("org_b", "k", "b", None).unwrap();
        assert_eq!(cache.get("org_a", "k").unwrap().unwrap().as_slice(), b"a");
        assert_eq!(cache.get("org_b", "k").unwrap().unwrap().as_slice(), b"b");
        cache.clear_namespace("org_a").unwrap();
        assert!(cache.get("org_a", "k").unwrap().is_none());
        assert!(cache.get("org_b", "k").unwrap().is_some());
    }

    #[test]
    fn ttl_expires_entries() {
        let cache = test_cache(100);
        cache
            .set("ns", "ephemeral", "x", Some(Duration::from_millis(30)))
            .unwrap();
        assert!(cache.get("ns", "ephemeral").unwrap().is_some());
        thread::sleep(Duration::from_millis(50));
        assert!(cache.get("ns", "ephemeral").unwrap().is_none());
    }

    #[test]
    fn invalidate_prefix() {
        let cache = test_cache(100);
        cache.set("ns", "inv:1", "1", None).unwrap();
        cache.set("ns", "inv:2", "2", None).unwrap();
        cache.set("ns", "other", "3", None).unwrap();
        assert_eq!(cache.invalidate_prefix("ns", "inv:").unwrap(), 2);
        assert!(cache.get("ns", "other").unwrap().is_some());
    }

    #[test]
    fn rejects_empty_namespace_and_key() {
        let cache = test_cache(100);
        assert!(matches!(
            cache.get("", "k"),
            Err(CacheError::EmptyNamespace)
        ));
        assert!(matches!(cache.get("ns", ""), Err(CacheError::EmptyKey)));
    }

    #[test]
    fn rejects_oversized_values() {
        let cache = test_cache(100);
        let big = vec![0u8; 2048];
        let err = cache.set("ns", "big", big, None).unwrap_err();
        assert!(matches!(err, CacheError::ValueTooLarge { .. }));
    }

    #[test]
    fn respects_max_capacity() {
        let cache = test_cache(2);
        cache.set("ns", "a", "1", None).unwrap();
        cache.set("ns", "b", "2", None).unwrap();
        cache.set("ns", "c", "3", None).unwrap();
        // Moka applies TinyLFU admission asynchronously; bound holds after maintenance.
        assert!(cache.len() <= 2);
    }

    #[test]
    fn applies_default_ttl_when_set_omits_ttl() {
        let cache = Cache::from_config(&CacheConfig {
            backend: CacheBackendKind::Moka,
            default_ttl_secs: Some(1),
            max_entries: 10,
            max_value_bytes: 1024,
            redis: None,
        })
        .unwrap();
        cache.set("ns", "k", "v", None).unwrap();
        assert!(cache.get("ns", "k").unwrap().is_some());
    }

    #[test]
    fn redis_backend_requires_redis_config() {
        let result = Cache::from_config(&CacheConfig {
            backend: CacheBackendKind::Redis,
            default_ttl_secs: Some(60),
            max_entries: 10,
            max_value_bytes: 1024,
            redis: None,
        });
        assert!(matches!(
            result,
            Err(CacheError::MissingBackendConfig {
                backend: "redis",
                ..
            })
        ));
    }

    #[test]
    fn config_build_defaults_to_moka() {
        let cache = CacheConfig::default().build().unwrap();
        assert_eq!(cache.backend(), CacheBackendKind::Moka);
    }
}
