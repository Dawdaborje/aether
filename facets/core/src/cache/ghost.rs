//! Ghost directory: metadata for recently evicted keys (ARC B1/B2 spirit).

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use super::labels::{CacheKind, CacheLabel};

#[derive(Debug, Clone)]
pub struct GhostEntry {
    pub kind: CacheKind,
    pub model: Option<String>,
    pub bytes_was: usize,
    pub evicted_at_unix: u64,
    pub org: Option<String>,
    pub plugin: Option<String>,
    seq: u64,
}

#[derive(Debug)]
pub struct GhostDirectory {
    capacity: usize,
    inner: Mutex<GhostInner>,
}

#[derive(Debug, Default)]
struct GhostInner {
    by_key: HashMap<String, GhostEntry>,
    /// Insertion order with lazy deletion: valid while `by_key[key].seq == seq`.
    order: VecDeque<(String, u64)>,
    /// `list:` ghosts per (plugin, model): the scan-pollution signal, kept incrementally.
    list_counts: HashMap<(Option<String>, Option<String>), u32>,
    seq: u64,
}

impl GhostInner {
    fn forget(&mut self, key: &str) -> Option<GhostEntry> {
        let entry = self.by_key.remove(key)?;
        if entry.kind == CacheKind::List {
            let slot = (entry.plugin.clone(), entry.model.clone());
            if let Some(n) = self.list_counts.get_mut(&slot) {
                *n = n.saturating_sub(1);
                if *n == 0 {
                    self.list_counts.remove(&slot);
                }
            }
        }
        Some(entry)
    }
}

impl GhostDirectory {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            inner: Mutex::new(GhostInner::default()),
        }
    }

    pub fn remember(&self, fq_key: &str, label: &CacheLabel, bytes_was: usize) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if guard.by_key.contains_key(fq_key) {
            return;
        }
        while guard.by_key.len() >= self.capacity {
            let Some((old, seq)) = guard.order.pop_front() else {
                break;
            };
            if guard.by_key.get(&old).is_some_and(|e| e.seq == seq) {
                guard.forget(&old);
            }
        }
        // Lazy-deleted entries pile up in `order`; keep it proportional to the live set.
        if guard.order.len() > self.capacity * 2 + 64 {
            let GhostInner { by_key, order, .. } = &mut *guard;
            order.retain(|(k, s)| by_key.get(k).is_some_and(|e| e.seq == *s));
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        guard.seq += 1;
        let seq = guard.seq;
        guard.order.push_back((fq_key.to_string(), seq));
        if label.kind == CacheKind::List {
            *guard
                .list_counts
                .entry((label.plugin.clone(), label.model.clone()))
                .or_default() += 1;
        }
        guard.by_key.insert(
            fq_key.to_string(),
            GhostEntry {
                kind: label.kind,
                model: label.model.clone(),
                bytes_was,
                evicted_at_unix: now,
                org: label.org.clone(),
                plugin: label.plugin.clone(),
                seq,
            },
        );
    }

    /// Returns the ghost if the key was in the directory (and removes it).
    pub fn take(&self, fq_key: &str) -> Option<GhostEntry> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .forget(fq_key)
    }

    pub fn contains(&self, fq_key: &str) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .by_key
            .contains_key(fq_key)
    }

    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .by_key
            .len()
    }

    /// True when many `list:` ghosts share this plugin/model (scan-pollution signal). O(1).
    pub fn list_pollution(&self, label: &CacheLabel) -> bool {
        if label.kind != CacheKind::List {
            return false;
        }
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard
            .list_counts
            .get(&(label.plugin.clone(), label.model.clone()))
            .is_some_and(|n| *n >= 3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pollution_counts_follow_remember_and_take() {
        let dir = GhostDirectory::new(16);
        let keys: Vec<String> = (0..3)
            .map(|i| format!("plugin:acme:crm::list:contact:{i}"))
            .collect();
        for k in &keys {
            dir.remember(k, &CacheLabel::parse(k), 10);
        }
        let label = CacheLabel::parse(&keys[0]);
        assert!(dir.list_pollution(&label));
        dir.take(&keys[0]);
        assert!(!dir.list_pollution(&label));
    }

    #[test]
    fn capacity_is_respected() {
        let dir = GhostDirectory::new(4);
        for i in 0..50u32 {
            let k = format!("plugin:acme:crm::row:contact:{i}");
            dir.remember(&k, &CacheLabel::parse(&k), 1);
            dir.take(&format!(
                "plugin:acme:crm::row:contact:{}",
                i.saturating_sub(1)
            ));
        }
        assert!(dir.len() <= 4);
    }
}
