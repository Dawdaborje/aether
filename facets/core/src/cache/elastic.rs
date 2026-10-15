//! Elastic Mantle byte target under pressure (Phase 2).

use std::sync::atomic::{AtomicU64, Ordering};

use super::stats::MantleStats;

/// Shrinks the live-map target when ghost hits spike (scan pollution signal),
/// then grows back toward the configured maximum.
pub struct ElasticTarget {
    max_bytes: u64,
    min_bytes: u64,
    target_bytes: AtomicU64,
}

impl ElasticTarget {
    pub fn new(max_bytes: u64) -> Self {
        let max_bytes = max_bytes.max(1024);
        let min_bytes = (max_bytes / 2).max(1024);
        Self {
            max_bytes,
            min_bytes,
            target_bytes: AtomicU64::new(max_bytes),
        }
    }

    pub fn target_bytes(&self) -> u64 {
        self.target_bytes.load(Ordering::Relaxed)
    }

    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    /// Adjust target from Mantle counters. Call from the reclaim loop.
    pub fn sample(&self, stats: &MantleStats) {
        let snap = stats.counters.snapshot();
        let lookups = snap.hits.saturating_add(snap.misses).max(1);
        let ghost_ratio = snap.ghost_hits as f64 / lookups as f64;
        let used = snap.bytes_used;
        let target = self.target_bytes();

        let under_pressure = used >= target.saturating_mul(85) / 100;
        let polluted = ghost_ratio >= 0.08;

        if under_pressure && polluted {
            let next = (target.saturating_mul(85) / 100).max(self.min_bytes);
            self.target_bytes.store(next, Ordering::Relaxed);
        } else if used < target.saturating_mul(60) / 100 && ghost_ratio < 0.03 {
            let next = (target.saturating_add(target / 10)).min(self.max_bytes);
            self.target_bytes.store(next, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_at_max() {
        let elastic = ElasticTarget::new(10_000);
        assert_eq!(elastic.target_bytes(), 10_000);
        assert!(elastic.min_bytes <= 10_000);
    }
}
