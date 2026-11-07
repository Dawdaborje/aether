//! Moka L1 + optional L2 (Redis or Memcached) demote/promote (Phase 2).

use std::sync::Arc;
use std::time::Duration;

use super::error::CacheError;
use super::moka::{DemoteJob, DemoteSender, MokaCache};
use super::store::{CacheStore, CacheValue};

/// L1 Mantle (Moka) with an optional shared L2. Size-evicted L1 entries are demoted
/// asynchronously; L2 hits are promoted back into L1.
pub struct TieredCache {
    l1: MokaCache,
    l2: Option<Arc<dyn CacheStore>>,
    /// The L2 worker's queue, for work that must not block the caller.
    l2_jobs: Option<DemoteSender>,
}

impl TieredCache {
    pub fn new(
        l1: MokaCache,
        l2: Option<Arc<dyn CacheStore>>,
        l2_jobs: Option<DemoteSender>,
    ) -> Self {
        Self { l1, l2, l2_jobs }
    }

    /// Drop the L2 copy of `key` without waiting on the network when the worker has room.
    fn forget_l2(&self, key: &str) {
        let Some(l2) = &self.l2 else {
            return;
        };
        if let Some(tx) = &self.l2_jobs
            && tx.try_send(DemoteJob::Delete(key.to_string())).is_ok()
        {
            return;
        }
        // Queue full or no worker: stay correct, pay the round trip.
        let _ = l2.delete(key);
    }
}

impl CacheStore for TieredCache {
    fn get(&self, key: &str) -> Result<Option<CacheValue>, CacheError> {
        if let Some(value) = self.l1.get(key)? {
            return Ok(Some(value));
        }
        let Some(l2) = &self.l2 else {
            return Ok(None);
        };
        let Some(value) = l2.get(key)? else {
            return Ok(None);
        };
        // Promote into L1 with a soft TTL so thrash does not pin L2 forever.
        let _ = self
            .l1
            .set(key, value.clone(), Some(Duration::from_secs(3600)));
        Ok(Some(value))
    }

    fn set(&self, key: &str, value: CacheValue, ttl: Option<Duration>) -> Result<(), CacheError> {
        self.l1.set(key, value, ttl)?;
        // Drop a stale L2 copy so the next miss does not resurrect old bytes.
        self.forget_l2(key);
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<bool, CacheError> {
        let l1 = self.l1.delete(key)?;
        let l2 = if let Some(l2) = &self.l2 {
            l2.delete(key)?
        } else {
            false
        };
        Ok(l1 || l2)
    }

    fn delete_prefix(&self, prefix: &str) -> Result<usize, CacheError> {
        let mut n = self.l1.delete_prefix(prefix)?;
        if let Some(l2) = &self.l2 {
            n = n.saturating_add(l2.delete_prefix(prefix)?);
        }
        Ok(n)
    }

    fn clear(&self) -> Result<(), CacheError> {
        self.l1.clear()?;
        if let Some(l2) = &self.l2 {
            l2.clear()?;
        }
        Ok(())
    }

    fn len(&self) -> usize {
        self.l1.len()
    }

    fn maintain(&self, target_bytes: Option<u64>) -> usize {
        self.l1.maintain(target_bytes)
    }
}
