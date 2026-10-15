//! Soft in-use pins: keys touched recently are skipped by reclaim.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Keys accessed within `ttl` are treated as in-use (request-path hot).
pub struct InUsePins {
    ttl: Duration,
    capacity: usize,
    inner: Mutex<HashMap<String, Instant>>,
}

impl InUsePins {
    pub fn new(ttl: Duration, capacity: usize) -> Self {
        Self {
            ttl: ttl.max(Duration::from_millis(100)),
            capacity: capacity.max(64),
            inner: Mutex::new(HashMap::new()),
        }
    }

    pub fn touch(&self, fq_key: &str) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        match guard.get_mut(fq_key) {
            Some(at) => *at = now,
            None => {
                guard.insert(fq_key.to_string(), now);
            }
        }
        if guard.len() > self.capacity {
            let cutoff = now.checked_sub(self.ttl).unwrap_or(now);
            guard.retain(|_, at| *at >= cutoff);
            if guard.len() > self.capacity {
                // Drop oldest.
                let mut keys: Vec<_> = guard.iter().map(|(k, at)| (k.clone(), *at)).collect();
                keys.sort_by_key(|(_, at)| *at);
                let drop_n = guard.len() - self.capacity;
                for (k, _) in keys.into_iter().take(drop_n) {
                    guard.remove(&k);
                }
            }
        }
    }

    pub fn is_hot(&self, fq_key: &str) -> bool {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let Some(at) = guard.get(fq_key).copied() else {
            return false;
        };
        if at.elapsed() >= self.ttl {
            guard.remove(fq_key);
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hot_then_cold() {
        // Constructor clamps TTL to ≥100ms.
        let pins = InUsePins::new(Duration::from_millis(100), 32);
        pins.touch("kernel::session:abc");
        assert!(pins.is_hot("kernel::session:abc"));
        std::thread::sleep(Duration::from_millis(120));
        assert!(!pins.is_hot("kernel::session:abc"));
    }
}
