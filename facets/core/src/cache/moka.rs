use std::sync::Arc;
use std::time::{Duration, Instant};

use moka::Expiry;
use moka::notification::RemovalCause;
use moka::sync::Cache as MokaInner;

use super::adaptive::AdaptiveLists;
use super::config::CacheConfig;
use super::error::CacheError;
use super::ghost::GhostDirectory;
use super::heat::HeatLedger;
use super::in_use::InUsePins;
use super::labels::CacheLabel;
use super::policy;
use super::stats::MantleStats;
use super::store::{CacheStore, CacheValue};

/// Work for the L2 worker thread, so the request path never waits on the network.
pub enum DemoteJob {
    /// A size-evicted L1 entry is kept in L2.
    Set {
        key: String,
        value: CacheValue,
        ttl: Option<Duration>,
    },
    /// L1 holds a newer value; drop the L2 copy.
    Delete(String),
}

pub type DemoteSender = std::sync::mpsc::SyncSender<DemoteJob>;

#[derive(Clone)]
struct TimedValue {
    value: CacheValue,
    ttl: Option<Duration>,
}

/// Per-entry TTL drawn from the value written at insert/update time.
struct PerEntryExpiry;

impl Expiry<String, TimedValue> for PerEntryExpiry {
    fn expire_after_create(
        &self,
        _key: &String,
        value: &TimedValue,
        _current_time: Instant,
    ) -> Option<Duration> {
        value.ttl
    }

    fn expire_after_update(
        &self,
        _key: &String,
        value: &TimedValue,
        _current_time: Instant,
        _current_duration: Option<Duration>,
    ) -> Option<Duration> {
        value.ttl
    }
}

/// In-process Mantle store backed by [Moka](https://docs.rs/moka).
pub struct MokaCache {
    inner: MokaInner<String, TimedValue>,
    max_value_bytes: usize,
    stats: Arc<MantleStats>,
    ghosts: Arc<GhostDirectory>,
    in_use: Arc<InUsePins>,
    adaptive: Arc<AdaptiveLists>,
    heat: Arc<HeatLedger>,
}

impl MokaCache {
    pub fn new(
        config: &CacheConfig,
        stats: Arc<MantleStats>,
        ghosts: Arc<GhostDirectory>,
        heat: Arc<HeatLedger>,
        in_use: Arc<InUsePins>,
        adaptive: Arc<AdaptiveLists>,
        demote: Option<DemoteSender>,
    ) -> Self {
        let stats_evict = Arc::clone(&stats);
        let ghosts_evict = Arc::clone(&ghosts);
        let adaptive_evict = Arc::clone(&adaptive);
        let max_bytes = config.max_bytes.max(config.max_entries as u64).max(1);

        let inner = MokaInner::builder()
            .max_capacity(max_bytes)
            .weigher(move |key: &String, value: &TimedValue| -> u32 {
                policy::entry_weight(key.as_str(), value.value.len())
            })
            .expire_after(PerEntryExpiry)
            .support_invalidation_closures()
            .eviction_listener(move |key, value, cause| {
                let label = CacheLabel::parse(key.as_str());
                let bytes = policy::entry_cost(key.as_str(), value.value.len()) as usize;
                match cause {
                    RemovalCause::Size | RemovalCause::Expired => {
                        ghosts_evict.remember(key.as_str(), &label, bytes);
                        adaptive_evict.on_evict(key.as_str());
                        stats_evict.record_remove(&label, bytes, true);
                        if cause == RemovalCause::Size {
                            if let Some(tx) = demote.as_ref() {
                                let key_part =
                                    key.split_once("::").map(|(_, k)| k).unwrap_or(key.as_str());
                                if !policy::is_tier_a_pinned(&label, key_part) {
                                    let _ = tx.try_send(DemoteJob::Set {
                                        key: key.as_ref().clone(),
                                        value: value.value.clone(),
                                        ttl: value.ttl.or(Some(Duration::from_secs(3600))),
                                    });
                                }
                            }
                        }
                    }
                    RemovalCause::Explicit | RemovalCause::Replaced => {
                        stats_evict.record_remove(&label, bytes, false);
                    }
                }
            })
            .build();

        Self {
            inner,
            max_value_bytes: config.max_value_bytes.max(1),
            stats,
            ghosts,
            in_use,
            adaptive,
            heat,
        }
    }

    fn validate_value(&self, value: &CacheValue) -> Result<(), CacheError> {
        if value.len() > self.max_value_bytes {
            return Err(CacheError::ValueTooLarge {
                max: self.max_value_bytes,
            });
        }
        Ok(())
    }
}

impl CacheStore for MokaCache {
    fn get(&self, key: &str) -> Result<Option<CacheValue>, CacheError> {
        let label = CacheLabel::parse(key);
        match self.inner.get(key) {
            Some(entry) => {
                self.stats.record_hit(&label, entry.value.len());
                self.in_use.touch(key);
                self.adaptive.touch(key);
                Ok(Some(entry.value))
            }
            None => {
                if self.ghosts.contains(key) {
                    self.stats.record_ghost_hit(&label);
                    self.adaptive.on_ghost_hit(key);
                }
                self.stats.record_miss(&label);
                Ok(None)
            }
        }
    }

    fn set(&self, key: &str, value: CacheValue, ttl: Option<Duration>) -> Result<(), CacheError> {
        self.validate_value(&value)?;
        let label = CacheLabel::parse(key);
        let bytes = policy::entry_cost(key, value.len()) as usize;
        if self.ghosts.take(key).is_some() {
            self.stats.record_ghost_hit(&label);
            self.adaptive.on_ghost_hit(key);
        }
        self.inner
            .insert(key.to_string(), TimedValue { value, ttl });
        self.stats.record_insert(&label, bytes);
        self.in_use.touch(key);
        self.adaptive.touch(key);
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<bool, CacheError> {
        let existed = self.inner.contains_key(key);
        self.inner.invalidate(key);
        self.inner.run_pending_tasks();
        Ok(existed)
    }

    fn delete_prefix(&self, prefix: &str) -> Result<usize, CacheError> {
        let keys: Vec<String> = self
            .inner
            .iter()
            .filter_map(|(k, _)| {
                if k.starts_with(prefix) {
                    Some(k.as_ref().clone())
                } else {
                    None
                }
            })
            .collect();

        let count = keys.len();
        for key in keys {
            self.inner.invalidate(&key);
        }
        self.inner.run_pending_tasks();
        Ok(count)
    }

    fn clear(&self) -> Result<(), CacheError> {
        self.inner.invalidate_all();
        self.inner.run_pending_tasks();
        Ok(())
    }

    fn len(&self) -> usize {
        self.inner.run_pending_tasks();
        self.inner.entry_count() as usize
    }

    fn maintain(&self, target_bytes: Option<u64>) -> usize {
        self.inner.run_pending_tasks();
        let Some(target) = target_bytes else {
            return 0;
        };
        let used = self
            .stats
            .counters
            .bytes_used
            .load(std::sync::atomic::Ordering::Relaxed);
        let budget = policy::reclaim_budget(used, target);
        if budget.max_bytes == 0 || budget.max_keys == 0 {
            return 0;
        }

        // (key, dignity, polluted-list, heat, adaptive priority, cost)
        let mut candidates: Vec<(String, u8, bool, f64, u8, u64)> = self
            .inner
            .iter()
            .filter_map(|(k, v)| {
                if self.in_use.is_hot(k.as_str()) {
                    return None;
                }
                let label = CacheLabel::parse(k.as_str());
                let key = k
                    .split_once("::")
                    .map(|(_, rest)| rest)
                    .unwrap_or(k.as_str());
                if !policy::is_reclaim_candidate(&label, key) {
                    return None;
                }
                let dig = label.kind.dignity();
                let polluted = self.ghosts.list_pollution(&label);
                let heat = policy::heat_score(&label, &self.heat);
                let adapt = self.adaptive.reclaim_priority(k.as_str());
                let bytes = policy::entry_cost(k.as_str(), v.value.len());
                Some((k.as_ref().clone(), dig, polluted, heat, adapt, bytes))
            })
            .collect();
        // Lowest dignity first; within a kind, scan-polluting lists, then the coldest
        // plugin/model, then the entry the adaptive lists would give up first.
        candidates.sort_by(|a, b| {
            a.1.cmp(&b.1)
                .then_with(|| b.2.cmp(&a.2))
                .then_with(|| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal))
                .then_with(|| b.4.cmp(&a.4))
        });

        let mut dropped = 0usize;
        let mut freed = 0u64;
        for (key, _, _, _, _, bytes) in candidates {
            if dropped >= budget.max_keys || freed >= budget.max_bytes {
                break;
            }
            let used_now = self
                .stats
                .counters
                .bytes_used
                .load(std::sync::atomic::Ordering::Relaxed);
            if used_now <= target {
                break;
            }
            self.inner.invalidate(&key);
            dropped += 1;
            freed = freed.saturating_add(bytes);
        }
        self.inner.run_pending_tasks();
        dropped
    }
}
