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
    /// External Memcached server(s).
    Memcached,
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

/// Memcached connection settings. Required when [`CacheBackendKind::Memcached`] is selected.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct MemcachedCacheConfig {
    /// `host:port` of each server. Keys are spread over them by hash.
    pub servers: Vec<String>,
    /// Prefix applied to every key (keeps Aether keys apart on shared servers).
    #[serde(default = "default_redis_key_prefix")]
    pub key_prefix: String,
}

impl Default for MemcachedCacheConfig {
    fn default() -> Self {
        Self {
            servers: vec!["127.0.0.1:11211".to_string()],
            key_prefix: default_redis_key_prefix(),
        }
    }
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

/// Tunables for the cache facet / Mantle. Loaded from `[cache]` in `aether.toml` when present.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CacheConfig {
    #[serde(default)]
    pub backend: CacheBackendKind,
    /// Default TTL applied when a `set` does not specify one. `None` = no expiry.
    pub default_ttl_secs: Option<u64>,
    /// Soft max live entries for the Moka backend (ignored by Redis).
    #[serde(default = "default_max_entries")]
    pub max_entries: usize,
    /// Max total weighted bytes for the Moka backend (Mantle byte budget).
    #[serde(default = "default_max_bytes")]
    pub max_bytes: u64,
    /// Max bytes for a single value.
    #[serde(default = "default_max_value_bytes")]
    pub max_value_bytes: usize,
    /// Ghost directory capacity (evicted key metadata only).
    #[serde(default = "default_ghost_capacity")]
    pub ghost_capacity: usize,
    /// Half-life for Mantle heat score decay, in seconds.
    #[serde(default = "default_heat_half_life_secs")]
    pub heat_half_life_secs: u64,
    /// Compress values at or above this size (deflate). `0` disables.
    #[serde(default = "default_compress_min_bytes")]
    pub compress_min_bytes: usize,
    /// Announce table writes to other nodes through `mantle_signals`, so their L1 entries for
    /// that table go stale at once instead of at TTL. Costs one coalesced insert per burst.
    #[serde(default = "default_true")]
    pub coherence_signals: bool,
    /// Redis settings. Used when `backend = "redis"`, or as L2 when `backend = "moka"`.
    #[serde(default)]
    pub redis: Option<RedisCacheConfig>,
    /// Memcached settings. Used when `backend = "memcached"`, or as L2 when `backend = "moka"`
    /// and no Redis is configured.
    #[serde(default)]
    pub memcached: Option<MemcachedCacheConfig>,
    /// App directory root for Mantle heat files. Set by the kernel when building the cache.
    #[serde(skip)]
    pub app_dir: Option<std::path::PathBuf>,
}

fn default_true() -> bool {
    true
}

fn default_max_entries() -> usize {
    10_000
}

fn default_max_bytes() -> u64 {
    64 * 1024 * 1024 // 64 MiB
}

fn default_max_value_bytes() -> usize {
    1024 * 1024 // 1 MiB
}

fn default_ghost_capacity() -> usize {
    4_096
}

fn default_heat_half_life_secs() -> u64 {
    12 * 3600
}

fn default_compress_min_bytes() -> usize {
    4 * 1024
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            backend: CacheBackendKind::Moka,
            default_ttl_secs: Some(300),
            max_entries: default_max_entries(),
            max_bytes: default_max_bytes(),
            max_value_bytes: default_max_value_bytes(),
            ghost_capacity: default_ghost_capacity(),
            heat_half_life_secs: default_heat_half_life_secs(),
            compress_min_bytes: default_compress_min_bytes(),
            coherence_signals: true,
            redis: None,
            memcached: None,
            app_dir: None,
        }
    }
}

impl CacheConfig {
    /// Build a ready-to-use [`crate::Cache`] from this config.
    pub fn build(&self) -> Result<super::Cache, super::CacheError> {
        super::Cache::from_config(self)
    }
}
