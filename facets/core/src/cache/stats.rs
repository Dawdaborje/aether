//! Mantle hit / byte / per-label rollups for the desk inspector.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use super::labels::{CacheKind, CacheLabel};

#[derive(Debug, Default)]
pub struct MantleCounters {
    pub hits: AtomicU64,
    pub misses: AtomicU64,
    pub ghost_hits: AtomicU64,
    pub evictions: AtomicU64,
    pub bytes_used: AtomicU64,
}

impl MantleCounters {
    pub fn snapshot(&self) -> CounterSnapshot {
        CounterSnapshot {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            ghost_hits: self.ghost_hits.load(Ordering::Relaxed),
            evictions: self.evictions.load(Ordering::Relaxed),
            bytes_used: self.bytes_used.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct CounterSnapshot {
    pub hits: u64,
    pub misses: u64,
    pub ghost_hits: u64,
    pub evictions: u64,
    pub bytes_used: u64,
}

#[derive(Debug, Clone, Default)]
pub struct LabelRollup {
    pub entries: u64,
    pub bytes: u64,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub ghost_hits: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RollupKey {
    pub org: String,
    pub plugin: String,
    pub kind: String,
    pub model: String,
}

impl RollupKey {
    pub fn from_label(label: &CacheLabel) -> Self {
        Self {
            org: label.org.clone().unwrap_or_default(),
            plugin: label.plugin.clone().unwrap_or_else(|| {
                if label.is_kernel {
                    "kernel".into()
                } else {
                    String::new()
                }
            }),
            kind: label.kind.as_str().into(),
            model: label.model.clone().unwrap_or_default(),
        }
    }
}

#[derive(Debug, Default)]
pub struct MantleStats {
    pub counters: MantleCounters,
    rollups: Mutex<HashMap<RollupKey, LabelRollup>>,
    kind_bytes: Mutex<HashMap<&'static str, u64>>,
}

impl MantleStats {
    pub fn record_hit(&self, label: &CacheLabel, bytes: usize) {
        self.counters.hits.fetch_add(1, Ordering::Relaxed);
        let mut rollups = self.rollups.lock().unwrap_or_else(|e| e.into_inner());
        let entry = rollups.entry(RollupKey::from_label(label)).or_default();
        entry.hits += 1;
        let _ = bytes;
    }

    pub fn record_miss(&self, label: &CacheLabel) {
        self.counters.misses.fetch_add(1, Ordering::Relaxed);
        let mut rollups = self.rollups.lock().unwrap_or_else(|e| e.into_inner());
        rollups
            .entry(RollupKey::from_label(label))
            .or_default()
            .misses += 1;
    }

    pub fn record_ghost_hit(&self, label: &CacheLabel) {
        self.counters.ghost_hits.fetch_add(1, Ordering::Relaxed);
        let mut rollups = self.rollups.lock().unwrap_or_else(|e| e.into_inner());
        rollups
            .entry(RollupKey::from_label(label))
            .or_default()
            .ghost_hits += 1;
    }

    pub fn record_insert(&self, label: &CacheLabel, bytes: usize) {
        self.counters
            .bytes_used
            .fetch_add(bytes as u64, Ordering::Relaxed);
        {
            let mut kind_bytes = self.kind_bytes.lock().unwrap_or_else(|e| e.into_inner());
            *kind_bytes.entry(label.kind.as_str()).or_default() += bytes as u64;
        }
        let mut rollups = self.rollups.lock().unwrap_or_else(|e| e.into_inner());
        let entry = rollups.entry(RollupKey::from_label(label)).or_default();
        entry.entries += 1;
        entry.bytes += bytes as u64;
    }

    pub fn record_remove(&self, label: &CacheLabel, bytes: usize, eviction: bool) {
        loop {
            let current = self.counters.bytes_used.load(Ordering::Relaxed);
            let next = current.saturating_sub(bytes as u64);
            if self
                .counters
                .bytes_used
                .compare_exchange(current, next, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                break;
            }
        }
        if eviction {
            self.counters.evictions.fetch_add(1, Ordering::Relaxed);
        }
        {
            let mut kind_bytes = self.kind_bytes.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(slot) = kind_bytes.get_mut(label.kind.as_str()) {
                *slot = slot.saturating_sub(bytes as u64);
            }
        }
        let mut rollups = self.rollups.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = rollups.get_mut(&RollupKey::from_label(label)) {
            entry.entries = entry.entries.saturating_sub(1);
            entry.bytes = entry.bytes.saturating_sub(bytes as u64);
            if eviction {
                entry.evictions += 1;
            }
        }
    }

    pub fn kind_bytes_snapshot(&self) -> HashMap<String, u64> {
        self.kind_bytes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|(k, v)| ((*k).to_string(), *v))
            .collect()
    }

    pub fn rollups_snapshot(&self) -> Vec<(RollupKey, LabelRollup)> {
        self.rollups
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    pub fn org_bytes(&self, org: &str) -> (u64, u64) {
        let mut bytes = 0u64;
        let mut entries = 0u64;
        for (key, rollup) in self
            .rollups
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
        {
            if key.org == org {
                bytes += rollup.bytes;
                entries += rollup.entries;
            }
        }
        (bytes, entries)
    }

    pub fn plugin_model_bytes(
        &self,
        org: &str,
        plugin: &str,
        model: Option<&str>,
        kind: Option<CacheKind>,
    ) -> (u64, u64) {
        let mut bytes = 0u64;
        let mut entries = 0u64;
        for (key, rollup) in self
            .rollups
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
        {
            if key.org != org || key.plugin != plugin {
                continue;
            }
            if let Some(model) = model
                && key.model != model
            {
                continue;
            }
            if let Some(kind) = kind
                && key.kind != kind.as_str()
            {
                continue;
            }
            bytes += rollup.bytes;
            entries += rollup.entries;
        }
        (bytes, entries)
    }

    pub fn kernel_vs_plugin_bytes(&self) -> (u64, u64) {
        let mut kernel = 0u64;
        let mut plugin = 0u64;
        for (key, rollup) in self
            .rollups
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
        {
            if key.kind == CacheKind::Kernel.as_str() || key.plugin == "kernel" {
                kernel += rollup.bytes;
            } else {
                plugin += rollup.bytes;
            }
        }
        (kernel, plugin)
    }
}
