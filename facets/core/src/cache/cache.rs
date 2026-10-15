use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use super::adaptive::AdaptiveLists;
use super::compress;
use super::config::{CacheBackendKind, CacheConfig};
use super::elastic::ElasticTarget;
use super::error::CacheError;
use super::ghost::GhostDirectory;
use super::heat::HeatLedger;
use super::in_use::InUsePins;
use super::labels::{
    CacheLabel, KERNEL_NAMESPACE, agg_key, kernel_org_namespace, kernel_session_key,
    list_key, meta_key, row_key,
};
use super::moka::{DemoteJob, MokaCache};
use super::memcached_store::MemcachedCache;
use super::redis_store::RedisCache;
use super::stats::MantleStats;
use super::store::{CacheStore, CacheValue, namespaced_key};
use super::tiered::TieredCache;
use super::warmup::{WarmupFetcher, WarmupHooks, WarmupStats};

/// Desk / ops snapshot of Mantle.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MantleStatsSnapshot {
    pub bytes_used: u64,
    pub bytes_max: u64,
    /// Elastic reclaim target (≤ `bytes_max`).
    pub bytes_target: u64,
    pub entries: u64,
    pub hits: u64,
    pub misses: u64,
    pub ghost_hits: u64,
    pub evictions: u64,
    pub backend: CacheBackendKind,
    pub l2_redis: bool,
    pub kernel_bytes: u64,
    pub plugin_bytes: u64,
    pub kind_bytes: std::collections::HashMap<String, u64>,
    pub ghost_len: usize,
    pub session_entries: u64,
    pub session_bytes: u64,
    /// Adaptive recent-list target in millis (0..=1000).
    pub adaptive_p_millis: u32,
    pub warmup: WarmupStats,
}

type FlightMap = std::sync::Mutex<
    std::collections::HashMap<String, Arc<tokio::sync::Mutex<()>>>,
>;

/// Held while one caller fills a missing entry; the others wait for it and then find the
/// entry in the cache instead of running the same query again. See [`Cache::flight`].
pub struct Flight {
    _guard: tokio::sync::OwnedMutexGuard<()>,
    map: Arc<FlightMap>,
    key: String,
}

impl Drop for Flight {
    fn drop(&mut self) {
        let mut map = self.map.lock().unwrap_or_else(|e| e.into_inner());
        // The map and this guard each hold one reference; anything more is a waiter.
        if map.get(&self.key).is_some_and(|l| Arc::strong_count(l) <= 2) {
            map.remove(&self.key);
        }
    }
}

/// Kernel-facing cache facade (Mantle). Plugin access goes through namespaced methods.
#[derive(Clone)]
pub struct Cache {
    store: Arc<dyn CacheStore>,
    stats: Arc<MantleStats>,
    ghosts: Arc<GhostDirectory>,
    heat: Arc<HeatLedger>,
    elastic: Arc<ElasticTarget>,
    adaptive: Arc<AdaptiveLists>,
    warmup: Arc<WarmupHooks>,
    default_ttl: Option<Duration>,
    max_value_bytes: usize,
    max_bytes: u64,
    compress_min_bytes: usize,
    backend: CacheBackendKind,
    l2_redis: bool,
    coherence_signals: bool,
    flights: Arc<FlightMap>,
}

impl Cache {
    pub fn new(
        store: Arc<dyn CacheStore>,
        stats: Arc<MantleStats>,
        ghosts: Arc<GhostDirectory>,
        heat: Arc<HeatLedger>,
        elastic: Arc<ElasticTarget>,
        adaptive: Arc<AdaptiveLists>,
        warmup: Arc<WarmupHooks>,
        config: &CacheConfig,
        l2_redis: bool,
    ) -> Self {
        Self {
            store,
            stats,
            ghosts,
            heat,
            elastic,
            adaptive,
            warmup,
            default_ttl: config.default_ttl_secs.map(Duration::from_secs),
            max_value_bytes: config.max_value_bytes,
            max_bytes: config.max_bytes,
            compress_min_bytes: config.compress_min_bytes,
            backend: config.backend,
            l2_redis,
            coherence_signals: config.coherence_signals,
            flights: Arc::default(),
        }
    }

    /// Serialize fills of one missing key. The first caller runs the query; callers that arrive
    /// meanwhile wait here, then re-check the cache and find what the first one stored.
    pub async fn flight(&self, key: &str) -> Flight {
        let lock = {
            let mut map = self.flights.lock().unwrap_or_else(|e| e.into_inner());
            map.entry(key.to_string()).or_default().clone()
        };
        Flight {
            _guard: lock.lock_owned().await,
            map: Arc::clone(&self.flights),
            key: key.to_string(),
        }
    }

    #[cfg(test)]
    pub(crate) fn flights_in_progress(&self) -> usize {
        self.flights.lock().unwrap().len()
    }

    /// Whether writes announce themselves to other nodes (`mantle_signals`).
    pub fn coherence_signals(&self) -> bool {
        self.coherence_signals
    }

    /// Build a cache from config.
    ///
    /// * `backend = moka` — L1 Mantle; when `[cache.redis]` is also set, Redis is L2 demote/promote.
    /// * `backend = redis` — Redis-only store (no L1).
    /// * `backend = memcached` — Memcached-only store (no L1).
    /// * `[cache.memcached]` with `backend = moka` and no Redis — Memcached is L2.
    pub fn from_config(config: &CacheConfig) -> Result<Self, CacheError> {
        let stats = Arc::new(MantleStats::default());
        let ghosts = Arc::new(GhostDirectory::new(config.ghost_capacity));
        let app_dir = config.app_dir.clone().unwrap_or_else(|| PathBuf::from("."));
        let heat = Arc::new(HeatLedger::new(
            app_dir,
            Some(config.heat_half_life_secs as f64),
        ));
        let elastic = Arc::new(ElasticTarget::new(config.max_bytes));
        let in_use = Arc::new(InUsePins::new(Duration::from_secs(2), 8_192));
        let adaptive = Arc::new(AdaptiveLists::new(config.ghost_capacity.max(256)));
        let warmup = Arc::new(WarmupHooks::new());

        let (store, l2_redis): (Arc<dyn CacheStore>, bool) = match config.backend {
            CacheBackendKind::Moka => {
                // L2 is Redis when configured, otherwise Memcached when configured.
                let l2: Option<Arc<dyn CacheStore>> = match (&config.redis, &config.memcached) {
                    (Some(redis), _) => Some(Arc::new(RedisCache::connect(
                        redis,
                        config.max_value_bytes,
                    )?)),
                    (None, Some(memcached)) => Some(Arc::new(MemcachedCache::connect(
                        memcached,
                        config.max_value_bytes,
                    )?)),
                    (None, None) => None,
                };
                let l2_tx = l2.as_ref().map(|l2| {
                    let (tx, rx) = std::sync::mpsc::sync_channel::<DemoteJob>(256);
                    let l2_worker = Arc::clone(l2);
                    let _ = std::thread::Builder::new()
                        .name("mantle-l2-worker".into())
                        .spawn(move || {
                            while let Ok(job) = rx.recv() {
                                match job {
                                    DemoteJob::Set { key, value, ttl } => {
                                        let _ = l2_worker.set(&key, value, ttl);
                                    }
                                    DemoteJob::Delete(key) => {
                                        let _ = l2_worker.delete(&key);
                                    }
                                }
                            }
                        });
                    tx
                });
                let l1 = MokaCache::new(
                    config,
                    Arc::clone(&stats),
                    Arc::clone(&ghosts),
                    Arc::clone(&heat),
                    Arc::clone(&in_use),
                    Arc::clone(&adaptive),
                    l2_tx.clone(),
                );
                let has_l2 = l2.is_some();
                (
                    Arc::new(TieredCache::new(l1, l2, l2_tx)) as Arc<dyn CacheStore>,
                    has_l2,
                )
            }
            CacheBackendKind::Redis => {
                let redis = config
                    .redis
                    .as_ref()
                    .ok_or(CacheError::MissingBackendConfig {
                        backend: "redis",
                        section: "cache.redis",
                    })?;
                (
                    Arc::new(RedisCache::connect(redis, config.max_value_bytes)?)
                        as Arc<dyn CacheStore>,
                    false,
                )
            }
            CacheBackendKind::Memcached => {
                let memcached = config
                    .memcached
                    .as_ref()
                    .ok_or(CacheError::MissingBackendConfig {
                        backend: "memcached",
                        section: "cache.memcached",
                    })?;
                (
                    Arc::new(MemcachedCache::connect(memcached, config.max_value_bytes)?)
                        as Arc<dyn CacheStore>,
                    false,
                )
            }
        };

        // Async reclaim: sample elastic target and drop low-dignity keys.
        {
            let store = Arc::clone(&store);
            let stats = Arc::clone(&stats);
            let elastic = Arc::clone(&elastic);
            let _ = std::thread::Builder::new()
                .name("mantle-reclaim".into())
                .spawn(move || {
                    loop {
                        std::thread::sleep(Duration::from_secs(5));
                        elastic.sample(&stats);
                        let _ = store.maintain(Some(elastic.target_bytes()));
                    }
                });
        }

        let cache = Self::new(
            store, stats, ghosts, heat, elastic, adaptive, warmup, config, l2_redis,
        );

        // Background warmup: refill top heat keys only when a fetcher is registered.
        {
            let cache_w = cache.clone();
            let _ = std::thread::Builder::new()
                .name("mantle-warmup".into())
                .spawn(move || {
                    loop {
                        std::thread::sleep(Duration::from_secs(2));
                        let _ = cache_w.warmup().drain(&cache_w, 16);
                    }
                });
        }

        Ok(cache)
    }

    pub fn backend(&self) -> CacheBackendKind {
        self.backend
    }

    pub fn max_value_bytes(&self) -> usize {
        self.max_value_bytes
    }

    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    pub fn stats(&self) -> Arc<MantleStats> {
        Arc::clone(&self.stats)
    }

    pub fn heat(&self) -> Arc<HeatLedger> {
        Arc::clone(&self.heat)
    }

    pub fn ghosts(&self) -> Arc<GhostDirectory> {
        Arc::clone(&self.ghosts)
    }

    pub fn len(&self) -> usize {
        self.store.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, namespace: &str, key: &str) -> Result<Option<CacheValue>, CacheError> {
        let fq = namespaced_key(namespace, key)?;
        let found = self.store.get(&fq)?.map(compress::unpack);
        if found.is_some() {
            self.record_heat(namespace, key);
        }
        Ok(found)
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
        if value.len() > self.max_value_bytes {
            return Err(CacheError::ValueTooLarge {
                max: self.max_value_bytes,
            });
        }
        let packed = compress::pack(value, self.compress_min_bytes);
        let ttl = ttl.or(self.default_ttl);
        self.store.set(&fq, packed, ttl)?;
        self.record_heat(namespace, key);
        Ok(())
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

    /// Drop the entire namespace (e.g. active org / plugin).
    pub fn clear_namespace(&self, namespace: &str) -> Result<usize, CacheError> {
        if namespace.is_empty() {
            return Err(CacheError::EmptyNamespace);
        }
        let prefix = format!("{namespace}::");
        self.store.delete_prefix(&prefix)
    }

    // --- Structured key helpers (plugin namespace) ---

    pub fn get_row(
        &self,
        namespace: &str,
        model: &str,
        id: &str,
    ) -> Result<Option<CacheValue>, CacheError> {
        self.get(namespace, &row_key(model, id))
    }

    pub fn set_row(
        &self,
        namespace: &str,
        model: &str,
        id: &str,
        value: impl Into<CacheValue>,
        ttl: Option<Duration>,
    ) -> Result<(), CacheError> {
        self.set(namespace, &row_key(model, id), value, ttl)
    }

    pub fn invalidate_row(
        &self,
        namespace: &str,
        model: &str,
        id: &str,
    ) -> Result<bool, CacheError> {
        self.invalidate(namespace, &row_key(model, id))
    }

    pub fn get_list(
        &self,
        namespace: &str,
        model: &str,
        query_hash: &str,
    ) -> Result<Option<CacheValue>, CacheError> {
        self.get(namespace, &list_key(model, query_hash))
    }

    pub fn set_list(
        &self,
        namespace: &str,
        model: &str,
        query_hash: &str,
        value: impl Into<CacheValue>,
        ttl: Option<Duration>,
    ) -> Result<(), CacheError> {
        self.set(namespace, &list_key(model, query_hash), value, ttl)
    }

    pub fn get_meta(&self, namespace: &str, name: &str) -> Result<Option<CacheValue>, CacheError> {
        self.get(namespace, &meta_key(name))
    }

    pub fn set_meta(
        &self,
        namespace: &str,
        name: &str,
        value: impl Into<CacheValue>,
        ttl: Option<Duration>,
    ) -> Result<(), CacheError> {
        self.set(namespace, &meta_key(name), value, ttl)
    }

    pub fn get_agg(
        &self,
        namespace: &str,
        model: &str,
        name: &str,
        scope_hash: &str,
    ) -> Result<Option<CacheValue>, CacheError> {
        self.get(namespace, &agg_key(model, name, scope_hash))
    }

    // --- Write generations (cheap, complete invalidation of derived entries) ---

    /// Monotonic write generation of `table` in `org`: the larger of the table's own counter and
    /// the organization-wide one. A missing counter is created from the clock, so a counter that
    /// was evicted can never repeat a value an old entry was stamped with.
    pub fn table_gen(&self, org: &str, table: &str) -> u64 {
        self.gen_value(org, &format!("stamp:{table}"))
            .max(self.gen_value(org, "stamp:*"))
    }

    /// A write happened to `table`: every entry stamped with an older generation is stale.
    pub fn bump_table_gen(&self, org: &str, table: &str) {
        self.gen_bump(org, &format!("stamp:{table}"));
    }

    /// A write happened that cannot be pinned to one table.
    pub fn bump_org_gen(&self, org: &str) {
        self.gen_bump(org, "stamp:*");
    }

    fn gen_value(&self, org: &str, key: &str) -> u64 {
        if let Ok(Some(value)) = self.kernel_org_get(org, key)
            && let Ok(text) = std::str::from_utf8(value.as_slice())
            && let Ok(stamp) = text.parse::<u64>()
        {
            return stamp;
        }
        let stamp = Self::gen_now();
        let _ = self.kernel_org_set(org, key, stamp.to_string(), None);
        stamp
    }

    fn gen_bump(&self, org: &str, key: &str) {
        let current = self.gen_value(org, key);
        let next = Self::gen_now().max(current.saturating_add(1));
        let _ = self.kernel_org_set(org, key, next.to_string(), None);
    }

    fn gen_now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1)
    }

    pub fn set_agg(
        &self,
        namespace: &str,
        model: &str,
        name: &str,
        scope_hash: &str,
        value: impl Into<CacheValue>,
        ttl: Option<Duration>,
    ) -> Result<(), CacheError> {
        self.set(namespace, &agg_key(model, name, scope_hash), value, ttl)
    }

    // --- Kernel warm-set / session (not clearable by plugins) ---

    pub fn kernel_get(&self, key: &str) -> Result<Option<CacheValue>, CacheError> {
        self.get(KERNEL_NAMESPACE, key)
    }

    pub fn kernel_set(
        &self,
        key: &str,
        value: impl Into<CacheValue>,
        ttl: Option<Duration>,
    ) -> Result<(), CacheError> {
        self.set(KERNEL_NAMESPACE, key, value, ttl)
    }

    pub fn kernel_invalidate(&self, key: &str) -> Result<bool, CacheError> {
        self.invalidate(KERNEL_NAMESPACE, key)
    }

    pub fn kernel_org_get(&self, org: &str, key: &str) -> Result<Option<CacheValue>, CacheError> {
        self.get(&kernel_org_namespace(org), key)
    }

    pub fn kernel_org_set(
        &self,
        org: &str,
        key: &str,
        value: impl Into<CacheValue>,
        ttl: Option<Duration>,
    ) -> Result<(), CacheError> {
        self.set(&kernel_org_namespace(org), key, value, ttl)
    }

    pub fn kernel_org_invalidate(&self, org: &str, key: &str) -> Result<bool, CacheError> {
        self.invalidate(&kernel_org_namespace(org), key)
    }

    pub fn session_get(&self, token_hash: &str) -> Result<Option<CacheValue>, CacheError> {
        self.kernel_get(&kernel_session_key(token_hash))
    }

    pub fn session_put(
        &self,
        token_hash: &str,
        value: impl Into<CacheValue>,
        ttl: Option<Duration>,
    ) -> Result<(), CacheError> {
        self.kernel_set(&kernel_session_key(token_hash), value, ttl)
    }

    pub fn session_invalidate(&self, token_hash: &str) -> Result<bool, CacheError> {
        self.kernel_invalidate(&kernel_session_key(token_hash))
    }

    pub fn mantle_snapshot(&self) -> MantleStatsSnapshot {
        let counters = self.stats.counters.snapshot();
        let (kernel_bytes, plugin_bytes) = self.stats.kernel_vs_plugin_bytes();
        // Sessions are the kernel entries whose family is `session`.
        let (session_bytes, session_entries) = self
            .stats
            .rollups_snapshot()
            .into_iter()
            .filter(|(k, _)| k.kind == "kernel" && k.model == "session")
            .fold((0u64, 0u64), |(bytes, entries), (_, r)| {
                (bytes + r.bytes, entries + r.entries)
            });
        MantleStatsSnapshot {
            bytes_used: counters.bytes_used,
            bytes_max: self.max_bytes,
            bytes_target: self.elastic.target_bytes(),
            entries: self.len() as u64,
            hits: counters.hits,
            misses: counters.misses,
            ghost_hits: counters.ghost_hits,
            evictions: counters.evictions,
            backend: self.backend,
            l2_redis: self.l2_redis,
            kernel_bytes,
            plugin_bytes,
            kind_bytes: self.stats.kind_bytes_snapshot(),
            ghost_len: self.ghosts.len(),
            session_entries,
            session_bytes,
            adaptive_p_millis: self.adaptive.p_millis(),
            warmup: self.warmup.snapshot_counts(),
        }
    }

    pub fn elastic(&self) -> Arc<ElasticTarget> {
        Arc::clone(&self.elastic)
    }

    pub fn warmup(&self) -> Arc<WarmupHooks> {
        Arc::clone(&self.warmup)
    }

    /// Register a plugin-scoped rehydrate callback for heat-driven warmup.
    pub fn register_row_warmer(&self, plugin: &str, fetcher: WarmupFetcher) {
        self.warmup.register(plugin, fetcher);
    }

    pub fn ensure_org_heat_loaded(&self, org: &str) {
        if self.heat.ensure_loaded(org) {
            self.schedule_warmup(org);
        }
    }

    /// Enqueue top warmable keys from heat for background refill.
    pub fn schedule_warmup(&self, org: &str) {
        let Some(snap) = self.heat.snapshot(org) else {
            return;
        };
        let pairs: Vec<(String, String)> = snap
            .top_keys
            .into_iter()
            .map(|k| (k.plugin, k.key))
            .collect();
        self.warmup.enqueue_from_heat(org, &pairs);
    }

    pub fn flush_heat(&self) -> Result<usize, std::io::Error> {
        self.heat.flush_dirty()
    }

    fn record_heat(&self, namespace: &str, key: &str) {
        let fq = format!("{namespace}::{key}");
        let label = CacheLabel::parse(&fq);
        let Some(org) = label.org.as_deref() else {
            return;
        };
        let Some(plugin) = label.plugin.as_deref() else {
            return;
        };
        // First access of an org in this process merges the persisted heat (before counting).
        if !self.heat.is_loaded(org) {
            self.ensure_org_heat_loaded(org);
        }
        self.heat.touch(org, plugin, &label, key);
    }
}
