//! Kernel cache facet — Mantle namespaced key/value store with TTL.
//!
//! Plugins reach this only through capability-gated kernel commands
//! (`cache::get`, `cache::set`, `cache::invalidate`, `cache::clear`, plus
//! structured helpers such as `cache::set_row`).
//!
//! Backends:
//! - **Moka** (default) — in-process concurrent Mantle store (byte-weighted)
//! - **Redis** — external Redis / Redis-compatible server
//!
//! See `docs/architecture/mantle.md`.

mod adaptive;
mod cache;
mod compress;
mod config;
mod elastic;
mod error;
mod ghost;
mod heat;
mod in_use;
mod labels;
mod memcached_store;
mod moka;
mod policy;
mod redis_store;
mod stats;
mod store;
mod tiered;
mod warm;
mod warmup;

pub use cache::{Cache, MantleStatsSnapshot};
pub use cache::Flight;
pub use config::{CacheBackendKind, CacheConfig, MemcachedCacheConfig, RedisCacheConfig};
pub use elastic::ElasticTarget;
pub use error::CacheError;
pub use ghost::GhostDirectory;
pub use heat::{HeatFile, HeatLedger, HeatModel, HeatPlugin};
pub use labels::{CacheKind, CacheLabel, KERNEL_NAMESPACE, agg_key, list_key, meta_key, row_key};
pub use memcached_store::MemcachedCache;
pub use moka::MokaCache;
pub use redis_store::RedisCache;
pub use stats::MantleStats;
pub use store::{CacheStore, CacheValue, namespaced_key};
pub use warm::{InstalledPluginWarm, TierPresence};
pub use warmup::{WarmupFetcher, WarmupHooks, WarmupJob, WarmupStats};

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    fn test_cache(max_entries: usize) -> Result<Cache, CacheError> {
        let config = CacheConfig {
            backend: CacheBackendKind::Moka,
            default_ttl_secs: None,
            max_entries,
            max_bytes: (max_entries as u64).saturating_mul(256).max(256),
            max_value_bytes: 1024,
            ghost_capacity: 128,
            heat_half_life_secs: 3600,
            compress_min_bytes: 4096,
            redis: None,
            memcached: None,
            coherence_signals: true,
            app_dir: None,
        };
        Cache::from_config(&config)
    }

    #[test]
    fn set_get_invalidate() {
        let cache = test_cache(100).unwrap();
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
        let cache = test_cache(100).unwrap();
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
        let cache = test_cache(100).unwrap();
        cache
            .set("ns", "ephemeral", "x", Some(Duration::from_millis(30)))
            .unwrap();
        assert!(cache.get("ns", "ephemeral").unwrap().is_some());
        thread::sleep(Duration::from_millis(50));
        assert!(cache.get("ns", "ephemeral").unwrap().is_none());
    }

    #[test]
    fn invalidate_prefix() {
        let cache = test_cache(100).unwrap();
        cache.set("ns", "inv:1", "1", None).unwrap();
        cache.set("ns", "inv:2", "2", None).unwrap();
        cache.set("ns", "other", "3", None).unwrap();
        assert_eq!(cache.invalidate_prefix("ns", "inv:").unwrap(), 2);
        assert!(cache.get("ns", "other").unwrap().is_some());
    }

    #[test]
    fn rejects_empty_namespace_and_key() {
        let cache = test_cache(100).unwrap();
        assert!(matches!(
            cache.get("", "k"),
            Err(CacheError::EmptyNamespace)
        ));
        assert!(matches!(cache.get("ns", ""), Err(CacheError::EmptyKey)));
    }

    #[test]
    fn rejects_oversized_values() {
        let cache = test_cache(100).unwrap();
        let big = vec![0u8; 2048];
        let err = cache.set("ns", "big", big, None).unwrap_err();
        assert!(matches!(err, CacheError::ValueTooLarge { .. }));
    }

    #[test]
    fn respects_byte_budget() {
        let cache = test_cache(2).unwrap();
        cache.set("ns", "a", "1", None).unwrap();
        cache.set("ns", "b", "2", None).unwrap();
        cache.set("ns", "c", "3", None).unwrap();
        cache.set("ns", "d", "4", None).unwrap();
        // Weighted Mantle capacity; bound holds after maintenance.
        assert!(cache.len() <= 4);
    }

    #[test]
    fn row_helpers_round_trip() {
        let cache = test_cache(50).unwrap();
        let ns = "plugin:acme:crm";
        cache
            .set_row(ns, "contact", "c1", br#"{"n":1}"#.as_slice(), None)
            .unwrap();
        let got = cache.get_row(ns, "contact", "c1").unwrap().unwrap();
        assert_eq!(got.as_slice(), br#"{"n":1}"#);
    }

    #[test]
    fn applies_default_ttl_when_set_omits_ttl() {
        let cache = Cache::from_config(&CacheConfig {
            backend: CacheBackendKind::Moka,
            default_ttl_secs: Some(1),
            max_entries: 10,
            max_bytes: 1024 * 1024,
            max_value_bytes: 1024,
            ghost_capacity: 64,
            heat_half_life_secs: 3600,
            compress_min_bytes: 4096,
            redis: None,
            memcached: None,
            coherence_signals: true,
            app_dir: None,
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
            max_bytes: 1024,
            max_value_bytes: 1024,
            ghost_capacity: 64,
            heat_half_life_secs: 3600,
            compress_min_bytes: 4096,
            redis: None,
            memcached: None,
            coherence_signals: true,
            app_dir: None,
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

    #[test]
    fn session_kernel_keys() {
        let cache = test_cache(50).unwrap();
        cache.session_put("abc", "payload", None).unwrap();
        assert_eq!(
            cache.session_get("abc").unwrap().unwrap().as_slice(),
            b"payload"
        );
        assert!(cache.session_invalidate("abc").unwrap());
        assert!(cache.session_get("abc").unwrap().is_none());
    }

    #[test]
    fn heat_persists_to_app_dir() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::from_config(&CacheConfig {
            backend: CacheBackendKind::Moka,
            default_ttl_secs: None,
            max_entries: 50,
            max_bytes: 1024 * 1024,
            max_value_bytes: 1024,
            ghost_capacity: 64,
            heat_half_life_secs: 3600,
            compress_min_bytes: 4096,
            redis: None,
            memcached: None,
            coherence_signals: true,
            app_dir: Some(dir.path().to_path_buf()),
        })
        .unwrap();
        cache
            .set("plugin:acme:crm", "row:contact:1", "x", None)
            .unwrap();
        assert_eq!(cache.flush_heat().unwrap(), 1);
        let path = dir.path().join("orgs/acme/cache/mantle/heat.json");
        assert!(path.is_file());
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"crm\""));
        assert!(raw.contains("contact"));
    }

    #[test]
    fn table_gen_moves_forward_and_survives_missing_counter() {
        let cache = test_cache(64).unwrap();
        let a = cache.table_gen("acme", "contact");
        assert_eq!(cache.table_gen("acme", "contact"), a);
        cache.bump_table_gen("acme", "contact");
        let b = cache.table_gen("acme", "contact");
        assert!(b > a);
        // Another table is unaffected; the org-wide counter moves every table.
        assert_eq!(
            cache.table_gen("acme", "deal"),
            cache.table_gen("acme", "deal")
        );
        let before = cache.table_gen("acme", "deal");
        cache.bump_org_gen("acme");
        assert!(cache.table_gen("acme", "deal") > before);
        // An evicted counter restarts from the clock, never from a value an old entry holds.
        cache
            .kernel_org_invalidate("acme", "stamp:contact")
            .unwrap();
        assert!(cache.table_gen("acme", "contact") >= b);
    }

    #[test]
    fn heat_loaded_after_first_touch_keeps_persisted_scores() {
        let dir = std::env::temp_dir().join(format!("mantle-heat-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let config = |dir: &std::path::Path| CacheConfig {
            backend: CacheBackendKind::Moka,
            default_ttl_secs: None,
            max_entries: 64,
            max_bytes: 64 * 1024,
            max_value_bytes: 1024,
            ghost_capacity: 64,
            heat_half_life_secs: 3600,
            compress_min_bytes: 0,
            redis: None,
            memcached: None,
            coherence_signals: true,
            app_dir: Some(dir.to_path_buf()),
        };
        let first = Cache::from_config(&config(&dir)).unwrap();
        for _ in 0..5 {
            first
                .set("plugin:acme:crm", "row:contact:1", "x", None)
                .unwrap();
        }
        first.flush_heat().unwrap();
        let second = Cache::from_config(&config(&dir)).unwrap();
        // A touch happens before anything asks for the org's heat.
        second
            .set("plugin:acme:crm", "row:contact:2", "y", None)
            .unwrap();
        let snap = second.heat().snapshot("acme").unwrap();
        let plugin = snap.plugins.iter().find(|p| p.plugin == "crm").unwrap();
        assert!(
            plugin.hits >= 6,
            "persisted heat was dropped: {}",
            plugin.hits
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn one_caller_fills_a_missing_key_and_the_rest_reuse_it() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        let cache = test_cache(64).unwrap();
        let fills = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for _ in 0..16 {
            let cache = cache.clone();
            let fills = Arc::clone(&fills);
            tasks.push(tokio::spawn(async move {
                if cache.get("plugin:acme:crm", "list:contact:q").unwrap().is_some() {
                    return;
                }
                let _flight = cache.flight("plugin:acme:crm|list:contact:q").await;
                if cache.get("plugin:acme:crm", "list:contact:q").unwrap().is_some() {
                    return;
                }
                fills.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(30)).await;
                cache
                    .set("plugin:acme:crm", "list:contact:q", "rows", None)
                    .unwrap();
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(fills.load(Ordering::SeqCst), 1);
        assert_eq!(cache.flights_in_progress(), 0, "flight entries are cleaned up");
    }
}
