use serde::{Deserialize, Serialize};

/// Which store backs the kernel cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CacheBackendKind {
    /// In-process cache backed by [moka](https://docs.rs/moka).
    #[default]
    Moka,
    /// External Redis (or Redis-compatible) server.
    Redis,
}

/// Redis connection settings. Required when [`CacheBackendKind::Redis`] is selected.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RedisCacheConfig {
    /// Connection URL, e.g. `redis://127.0.0.1:6379/0`.
    pub url: String,
    /// Prefix applied to every Redis key (keeps Aether keys namespaced on shared servers).
    #[serde(default = "default_redis_key_prefix")]
    pub key_prefix: String,
}

fn default_redis_key_prefix() -> String {
    "aether:cache:".to_string()
}

impl Default for RedisCacheConfig {
    fn default() -> Self {
        Self {
            url: "redis://127.0.0.1:6379".to_string(),
            key_prefix: default_redis_key_prefix(),
        }
    }
}

/// Tunables for the cache facet. Loaded from `[cache]` in `aether.toml` when present.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CacheConfig {
    #[serde(default)]
    pub backend: CacheBackendKind,
    /// Default TTL applied when a `set` does not specify one. `None` = no expiry.
    pub default_ttl_secs: Option<u64>,
    /// Max live entries for the Moka backend (ignored by Redis).
    #[serde(default = "default_max_entries")]
    pub max_entries: usize,
    /// Max bytes for a single value.
    #[serde(default = "default_max_value_bytes")]
    pub max_value_bytes: usize,
    /// Redis settings. Used when `backend = "redis"`.
    #[serde(default)]
    pub redis: Option<RedisCacheConfig>,
}

fn default_max_entries() -> usize {
    10_000
}

fn default_max_value_bytes() -> usize {
    1024 * 1024 // 1 MiB
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            backend: CacheBackendKind::Moka,
            default_ttl_secs: Some(300),
            max_entries: default_max_entries(),
            max_value_bytes: default_max_value_bytes(),
            redis: None,
        }
    }
}

impl CacheConfig {
    /// Build a ready-to-use [`crate::Cache`] from this config.
    pub fn build(&self) -> Result<crate::Cache, crate::CacheError> {
        crate::Cache::from_config(self)
    }
}
