//! Heat-driven row/meta warmup: enqueue top keys, refill only via registered fetchers.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::Cache;
use super::labels::CacheKind;
use super::store::CacheValue;

const MAX_QUEUE: usize = 512;
const MAX_PER_ORG: usize = 32;

/// One pending rehydrate request from heat `top_keys`.
#[derive(Debug, Clone)]
pub struct WarmupJob {
    pub org: String,
    pub plugin: String,
    pub key: String,
}

impl WarmupJob {
    pub fn namespace(&self) -> String {
        format!("plugin:{}:{}", self.org, self.plugin)
    }
}

/// Plugin-scoped rehydrate callback. Return `None` to skip; never invent silent DB reads
/// outside this hook.
pub type WarmupFetcher =
    Arc<dyn Fn(&WarmupJob) -> Result<Option<CacheValue>, String> + Send + Sync>;

#[derive(Debug, Default)]
struct Counters {
    enqueued: AtomicU64,
    filled: AtomicU64,
    skipped_present: AtomicU64,
    skipped_no_fetcher: AtomicU64,
    errors: AtomicU64,
}

/// Registry + queue for active row/meta warmup.
pub struct WarmupHooks {
    fetchers: Mutex<HashMap<String, WarmupFetcher>>,
    queue: Mutex<VecDeque<WarmupJob>>,
    counters: Counters,
}

impl WarmupHooks {
    pub fn new() -> Self {
        Self {
            fetchers: Mutex::new(HashMap::new()),
            queue: Mutex::new(VecDeque::new()),
            counters: Counters::default(),
        }
    }

    pub fn register(&self, plugin: &str, fetcher: WarmupFetcher) {
        if plugin.is_empty() {
            return;
        }
        self.fetchers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(plugin.to_string(), fetcher);
    }

    pub fn unregister(&self, plugin: &str) {
        self.fetchers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(plugin);
    }

    pub fn pending(&self) -> usize {
        self.queue.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn snapshot_counts(&self) -> WarmupStats {
        WarmupStats {
            pending: self.pending() as u64,
            enqueued: self.counters.enqueued.load(Ordering::Relaxed),
            filled: self.counters.filled.load(Ordering::Relaxed),
            skipped_present: self.counters.skipped_present.load(Ordering::Relaxed),
            skipped_no_fetcher: self.counters.skipped_no_fetcher.load(Ordering::Relaxed),
            errors: self.counters.errors.load(Ordering::Relaxed),
        }
    }

    /// Enqueue warmable keys from a heat snapshot (row + meta only).
    pub fn enqueue_from_heat(&self, org: &str, top_keys: &[(String, String)]) {
        let mut q = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        let mut added = 0usize;
        for (plugin, key) in top_keys {
            if added >= MAX_PER_ORG {
                break;
            }
            if !is_warmable_key(key) {
                continue;
            }
            if q.iter()
                .any(|j| j.org == org && j.plugin == *plugin && j.key == *key)
            {
                continue;
            }
            while q.len() >= MAX_QUEUE {
                q.pop_front();
            }
            q.push_back(WarmupJob {
                org: org.to_string(),
                plugin: plugin.clone(),
                key: key.clone(),
            });
            added += 1;
            self.counters.enqueued.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Drain up to `limit` jobs into `cache` via registered fetchers.
    pub fn drain(&self, cache: &Cache, limit: usize) -> usize {
        let mut filled = 0usize;
        for _ in 0..limit {
            let Some(job) = self.pop() else {
                break;
            };
            let ns = job.namespace();
            match cache.get(&ns, &job.key) {
                Ok(Some(_)) => {
                    self.counters
                        .skipped_present
                        .fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                Ok(None) => {}
                Err(_) => {
                    self.counters.errors.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
            }
            let fetcher = {
                let g = self.fetchers.lock().unwrap_or_else(|e| e.into_inner());
                g.get(&job.plugin).cloned()
            };
            let Some(fetcher) = fetcher else {
                self.counters
                    .skipped_no_fetcher
                    .fetch_add(1, Ordering::Relaxed);
                continue;
            };
            match fetcher(&job) {
                Ok(Some(value)) => {
                    if cache.set(&ns, &job.key, value, None).is_ok() {
                        self.counters.filled.fetch_add(1, Ordering::Relaxed);
                        filled += 1;
                    } else {
                        self.counters.errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
                Ok(None) => {}
                Err(_) => {
                    self.counters.errors.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        filled
    }

    fn pop(&self) -> Option<WarmupJob> {
        self.queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop_front()
    }
}

impl Default for WarmupHooks {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WarmupStats {
    pub pending: u64,
    pub enqueued: u64,
    pub filled: u64,
    pub skipped_present: u64,
    pub skipped_no_fetcher: u64,
    pub errors: u64,
}

fn is_warmable_key(key: &str) -> bool {
    let kind = match key.split(':').next() {
        Some("row") => CacheKind::Row,
        Some("meta") => CacheKind::Meta,
        _ => return false,
    };
    matches!(kind, CacheKind::Row | CacheKind::Meta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::config::{CacheBackendKind, CacheConfig};
    use std::sync::atomic::AtomicUsize;

    fn test_cache() -> Cache {
        Cache::from_config(&CacheConfig {
            backend: CacheBackendKind::Moka,
            default_ttl_secs: None,
            max_entries: 64,
            max_bytes: 64 * 256,
            max_value_bytes: 1024,
            ghost_capacity: 32,
            heat_half_life_secs: 3600,
            compress_min_bytes: 4096,
            redis: None,
            memcached: None,
            coherence_signals: true,
            app_dir: None,
        })
        .unwrap()
    }

    #[test]
    fn fetcher_fills_missing_row() {
        let cache = test_cache();
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_c = Arc::clone(&calls);
        cache.register_row_warmer(
            "crm",
            Arc::new(move |job| {
                calls_c.fetch_add(1, Ordering::Relaxed);
                assert_eq!(job.key, "row:contact:1");
                Ok(Some(CacheValue::new(br#"{"n":1}"#.as_slice())))
            }),
        );
        cache
            .warmup()
            .enqueue_from_heat("acme", &[("crm".into(), "row:contact:1".into())]);
        let filled = cache.warmup().drain(&cache, 8);
        assert_eq!(filled, 1);
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        let got = cache
            .get_row("plugin:acme:crm", "contact", "1")
            .unwrap()
            .unwrap();
        assert_eq!(got.as_slice(), br#"{"n":1}"#);
    }

    #[test]
    fn skips_without_fetcher() {
        let cache = test_cache();
        cache
            .warmup()
            .enqueue_from_heat("acme", &[("crm".into(), "row:contact:1".into())]);
        let filled = cache.warmup().drain(&cache, 8);
        assert_eq!(filled, 0);
        assert_eq!(cache.warmup().snapshot_counts().skipped_no_fetcher, 1);
    }
}
