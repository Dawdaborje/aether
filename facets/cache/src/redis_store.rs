use std::sync::Mutex;
use std::time::Duration;

use redis::{Client, Commands, Connection, RedisResult};

use crate::config::RedisCacheConfig;
use crate::error::CacheError;
use crate::store::{CacheStore, CacheValue};

/// External Redis (or Redis-compatible) cache backend.
pub struct RedisCache {
    client: Client,
    conn: Mutex<Connection>,
    key_prefix: String,
    max_value_bytes: usize,
}

impl RedisCache {
    pub fn connect(config: &RedisCacheConfig, max_value_bytes: usize) -> Result<Self, CacheError> {
        let client = Client::open(config.url.as_str())
            .map_err(|e| CacheError::Backend(format!("redis client: {e}")))?;
        let conn = client
            .get_connection()
            .map_err(|e| CacheError::Backend(format!("redis connect: {e}")))?;

        Ok(Self {
            client,
            conn: Mutex::new(conn),
            key_prefix: config.key_prefix.clone(),
            max_value_bytes: max_value_bytes.max(1),
        })
    }

    fn redis_key(&self, key: &str) -> String {
        format!("{}{key}", self.key_prefix)
    }

    fn with_conn<F, T>(&self, f: F) -> Result<T, CacheError>
    where
        F: FnOnce(&mut Connection) -> RedisResult<T>,
    {
        let mut guard = self
            .conn
            .lock()
            .map_err(|_| CacheError::Backend("redis connection lock poisoned".into()))?;

        // Reconnect before the operation if the pooled connection is dead.
        if let Err(err) = redis::cmd("PING").query::<String>(&mut *guard)
            && (err.is_connection_dropped() || err.is_io_error())
        {
            let fresh = self
                .client
                .get_connection()
                .map_err(|e| CacheError::Backend(format!("redis reconnect: {e}")))?;
            *guard = fresh;
        }

        f(&mut guard).map_err(|e| CacheError::Backend(e.to_string()))
    }

    fn validate_value(&self, value: &CacheValue) -> Result<(), CacheError> {
        if value.len() > self.max_value_bytes {
            return Err(CacheError::ValueTooLarge {
                max: self.max_value_bytes,
            });
        }
        Ok(())
    }

    fn scan_keys(&self, pattern: &str) -> Result<Vec<String>, CacheError> {
        let mut cursor: u64 = 0;
        let mut keys = Vec::new();

        loop {
            let (next, batch): (u64, Vec<String>) = self.with_conn(|conn| {
                redis::cmd("SCAN")
                    .arg(cursor)
                    .arg("MATCH")
                    .arg(pattern)
                    .arg("COUNT")
                    .arg(100)
                    .query(conn)
            })?;
            keys.extend(batch);
            cursor = next;
            if cursor == 0 {
                break;
            }
        }

        Ok(keys)
    }
}

impl CacheStore for RedisCache {
    fn get(&self, key: &str) -> Result<Option<CacheValue>, CacheError> {
        let redis_key = self.redis_key(key);
        let bytes: Option<Vec<u8>> = self.with_conn(|conn| conn.get(redis_key))?;
        Ok(bytes.map(CacheValue::new))
    }

    fn set(
        &self,
        key: &str,
        value: CacheValue,
        ttl: Option<Duration>,
    ) -> Result<(), CacheError> {
        self.validate_value(&value)?;
        let redis_key = self.redis_key(key);
        let bytes = value.bytes;

        self.with_conn(|conn| match ttl {
            Some(duration) => {
                let secs = duration.as_secs().max(1);
                conn.set_ex(redis_key, bytes, secs)
            }
            None => conn.set(redis_key, bytes),
        })
    }

    fn delete(&self, key: &str) -> Result<bool, CacheError> {
        let redis_key = self.redis_key(key);
        let removed: i32 = self.with_conn(|conn| conn.del(redis_key))?;
        Ok(removed > 0)
    }

    fn delete_prefix(&self, prefix: &str) -> Result<usize, CacheError> {
        let pattern = format!("{}{}*", self.key_prefix, prefix);
        let keys = self.scan_keys(&pattern)?;
        if keys.is_empty() {
            return Ok(0);
        }
        let n: i32 = self.with_conn(|conn| conn.del(keys))?;
        Ok(n as usize)
    }

    fn clear(&self) -> Result<(), CacheError> {
        let pattern = format!("{}*", self.key_prefix);
        let keys = self.scan_keys(&pattern)?;
        if keys.is_empty() {
            return Ok(());
        }
        let _: i32 = self.with_conn(|conn| conn.del(keys))?;
        Ok(())
    }

    fn len(&self) -> usize {
        let pattern = format!("{}*", self.key_prefix);
        self.scan_keys(&pattern).map(|k| k.len()).unwrap_or(0)
    }
}
