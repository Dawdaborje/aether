use std::time::{Duration, Instant};

use moka::Expiry;
use moka::sync::Cache as MokaInner;

use super::config::CacheConfig;
use super::error::CacheError;
use super::store::{CacheStore, CacheValue};

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

/// In-process cache backed by [Moka](https://docs.rs/moka). Default kernel backend.
pub struct MokaCache {
    inner: MokaInner<String, TimedValue>,
    max_value_bytes: usize,
}

impl MokaCache {
    pub fn new(config: &CacheConfig) -> Self {
        let inner = MokaInner::builder()
            .max_capacity(config.max_entries.max(1) as u64)
            .expire_after(PerEntryExpiry)
            .support_invalidation_closures()
            .build();

        Self {
            inner,
            max_value_bytes: config.max_value_bytes.max(1),
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
        Ok(self.inner.get(key).map(|entry| entry.value))
    }

    fn set(&self, key: &str, value: CacheValue, ttl: Option<Duration>) -> Result<(), CacheError> {
        self.validate_value(&value)?;
        self.inner
            .insert(key.to_string(), TimedValue { value, ttl });
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<bool, CacheError> {
        let existed = self.inner.contains_key(key);
        self.inner.invalidate(key);
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
}
