//! Recency / frequency shadow lists with an ARC-style adaptive target `p`.
//!
//! Every operation is O(1) amortised: lists are queues with lazy deletion (an entry is live only
//! while its sequence number matches the one in the index), so a touch never scans a list.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

/// Which shadow list a live (or recently ghosted) key belonged to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    Recent,
    Frequent,
}

/// A queue whose entries are valid only while `index[key] == seq`.
#[derive(Default)]
struct Lazy {
    queue: VecDeque<(String, u64)>,
    index: HashMap<String, u64>,
}

impl Lazy {
    fn len(&self) -> usize {
        self.index.len()
    }

    fn contains(&self, key: &str) -> bool {
        self.index.contains_key(key)
    }

    fn push(&mut self, key: &str, seq: u64) {
        self.index.insert(key.to_string(), seq);
        self.queue.push_back((key.to_string(), seq));
        // Stale entries are skipped on pop; compact when they dominate.
        if self.queue.len() > self.index.len() * 2 + 64 {
            let index = &self.index;
            self.queue
                .retain(|(k, s)| index.get(k).is_some_and(|cur| cur == s));
        }
    }

    fn remove(&mut self, key: &str) -> bool {
        self.index.remove(key).is_some()
    }

    /// Oldest live key.
    fn pop_oldest(&mut self) -> Option<String> {
        while let Some((key, seq)) = self.queue.pop_front() {
            if self.index.get(&key) == Some(&seq) {
                self.index.remove(&key);
                return Some(key);
            }
        }
        None
    }
}

struct Inner {
    /// T1 — recent one-hit wonders.
    recent: Lazy,
    /// T2 — seen again (frequency).
    frequent: Lazy,
    /// B1 / B2 ghost membership for adaptive `p`.
    ghost_recent: Lazy,
    ghost_frequent: Lazy,
    seq: u64,
    /// Target fraction of reclaim pressure aimed at the recent list (0..=1000).
    p_millis: u32,
    capacity: usize,
}

impl Inner {
    fn next_seq(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }

    fn ghost(&mut self, key: &str, kind: ListKind) {
        let seq = self.next_seq();
        match kind {
            ListKind::Recent => self.ghost_recent.push(key, seq),
            ListKind::Frequent => self.ghost_frequent.push(key, seq),
        }
        while self.ghost_recent.len() > self.capacity {
            if self.ghost_recent.pop_oldest().is_none() {
                break;
            }
        }
        while self.ghost_frequent.len() > self.capacity {
            if self.ghost_frequent.pop_oldest().is_none() {
                break;
            }
        }
    }

    fn trim(&mut self) {
        while self.recent.len() + self.frequent.len() > self.capacity {
            let recent_target = (self.capacity as u64 * u64::from(self.p_millis) / 1000) as usize;
            let drop_recent = self.recent.len() > 0
                && (self.frequent.len() == 0 || self.recent.len() > recent_target.max(1));
            let dropped = if drop_recent {
                self.recent.pop_oldest().map(|k| (k, ListKind::Recent))
            } else {
                self.frequent.pop_oldest().map(|k| (k, ListKind::Frequent))
            };
            match dropped {
                Some((key, kind)) => self.ghost(&key, kind),
                None => break,
            }
        }
    }
}

/// Shadow dual-list policy that biases reclaim without replacing Moka's TinyLFU.
pub struct AdaptiveLists {
    inner: Mutex<Inner>,
}

impl AdaptiveLists {
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(64);
        Self {
            inner: Mutex::new(Inner {
                recent: Lazy::default(),
                frequent: Lazy::default(),
                ghost_recent: Lazy::default(),
                ghost_frequent: Lazy::default(),
                seq: 0,
                p_millis: 500,
                capacity,
            }),
        }
    }

    /// Record a hit/insert. Second touch promotes recent → frequent.
    pub fn touch(&self, fq_key: &str) {
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        g.ghost_recent.remove(fq_key);
        g.ghost_frequent.remove(fq_key);
        let seq = g.next_seq();
        if g.frequent.contains(fq_key) {
            // Move to the MRU end of frequent.
            g.frequent.push(fq_key, seq);
            return;
        }
        if g.recent.remove(fq_key) {
            g.frequent.push(fq_key, seq);
        } else {
            g.recent.push(fq_key, seq);
        }
        g.trim();
    }

    /// Called when a key is size/TTL-evicted from L1.
    pub fn on_evict(&self, fq_key: &str) {
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let kind = if g.frequent.remove(fq_key) {
            ListKind::Frequent
        } else {
            g.recent.remove(fq_key);
            ListKind::Recent
        };
        g.ghost(fq_key, kind);
    }

    /// Ghost directory hit: adapt `p` (B1 → favor frequency, B2 → favor recency).
    pub fn on_ghost_hit(&self, fq_key: &str) {
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if g.ghost_recent.remove(fq_key) {
            // Miss that should have been recent → grow frequent preference (lower p).
            g.p_millis = g.p_millis.saturating_sub(50).max(50);
        } else if g.ghost_frequent.remove(fq_key) {
            // Miss that should have been frequent → grow recent preference (higher p).
            g.p_millis = (g.p_millis + 50).min(950);
        }
    }

    /// Higher score = reclaim sooner. Biased by adaptive `p`.
    pub fn reclaim_priority(&self, fq_key: &str) -> u8 {
        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let p = g.p_millis;
        if g.recent.contains(fq_key) {
            // When p is high, reclaim recent first.
            if p >= 500 { 10 } else { 4 }
        } else if g.frequent.contains(fq_key) {
            if p < 500 { 10 } else { 4 }
        } else {
            6
        }
    }

    pub fn p_millis(&self) -> u32 {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .p_millis
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_touch_promotes() {
        let lists = AdaptiveLists::new(32);
        lists.touch("a");
        lists.touch("a");
        assert_eq!(lists.reclaim_priority("a"), 4); // frequent, p default 500 → lower urgency
    }

    #[test]
    fn ghost_hit_adapts_p() {
        let lists = AdaptiveLists::new(32);
        lists.touch("recent-only");
        lists.on_evict("recent-only");
        let before = lists.p_millis();
        lists.on_ghost_hit("recent-only");
        assert!(lists.p_millis() < before);
    }

    #[test]
    fn capacity_is_bounded_and_touch_stays_cheap() {
        let lists = AdaptiveLists::new(64);
        for i in 0..10_000 {
            lists.touch(&format!("k{}", i % 500));
        }
        let g = lists.inner.lock().unwrap();
        assert!(g.recent.len() + g.frequent.len() <= 64);
        assert!(g.recent.queue.len() < 64 * 4 + 200);
        assert!(g.frequent.queue.len() < 64 * 4 + 200);
    }
}
