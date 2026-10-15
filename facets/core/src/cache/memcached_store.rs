//! Memcached backend (text protocol), usable standalone or as the L2 behind Moka.
//!
//! A small purpose-built client rather than a dependency: the cache only needs
//! `get` / `set` / `delete` / `incr`. Memcached cannot list or delete by prefix, so those
//! operations bump an *epoch* that is part of every stored key: everything written under
//! the old epoch becomes unreachable and expires on its own. That over-invalidates (a prefix
//! delete drops every entry of this key prefix, not only the prefix asked for) but is never
//! stale, which is the property a cache has to keep.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use super::config::MemcachedCacheConfig;
use super::error::CacheError;
use super::store::{CacheStore, CacheValue};

const IO_TIMEOUT: Duration = Duration::from_millis(500);
/// How long this process trusts its copy of the epoch. Bounds how long another node's prefix
/// delete can go unnoticed here.
const EPOCH_REFRESH: Duration = Duration::from_secs(1);
/// Memcached keys are at most 250 bytes of printable, non-space ASCII.
const MAX_KEY: usize = 200;

struct Conn {
    addr: String,
    stream: Option<BufReader<TcpStream>>,
}

impl Conn {
    fn stream(&mut self) -> Result<&mut BufReader<TcpStream>, CacheError> {
        if self.stream.is_none() {
            let addr = self
                .addr
                .to_socket_addrs()
                .map_err(|e| CacheError::Backend(format!("memcached address {}: {e}", self.addr)))?
                .next()
                .ok_or_else(|| CacheError::Backend(format!("memcached address {}", self.addr)))?;
            let stream = TcpStream::connect_timeout(&addr, IO_TIMEOUT)
                .map_err(|e| CacheError::Backend(format!("memcached connect {}: {e}", self.addr)))?;
            let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
            let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
            let _ = stream.set_nodelay(true);
            self.stream = Some(BufReader::new(stream));
        }
        Ok(self.stream.as_mut().expect("connected above"))
    }

    /// Run one request; a broken connection is dropped so the next call reconnects.
    fn run<T>(
        &mut self,
        f: impl FnOnce(&mut BufReader<TcpStream>) -> std::io::Result<T>,
    ) -> Result<T, CacheError> {
        let stream = self.stream()?;
        match f(stream) {
            Ok(value) => Ok(value),
            Err(error) => {
                self.stream = None;
                Err(CacheError::Backend(format!("memcached: {error}")))
            }
        }
    }
}

pub struct MemcachedCache {
    servers: Vec<Mutex<Conn>>,
    key_prefix: String,
    max_value_bytes: usize,
    epoch: Mutex<(u64, Instant)>,
}

impl MemcachedCache {
    pub fn connect(
        config: &MemcachedCacheConfig,
        max_value_bytes: usize,
    ) -> Result<Self, CacheError> {
        if config.servers.is_empty() {
            return Err(CacheError::MissingBackendConfig {
                backend: "memcached",
                section: "cache.memcached.servers",
            });
        }
        let cache = Self {
            servers: config
                .servers
                .iter()
                .map(|addr| {
                    Mutex::new(Conn {
                        addr: addr.clone(),
                        stream: None,
                    })
                })
                .collect(),
            key_prefix: config.key_prefix.clone(),
            max_value_bytes: max_value_bytes.max(1),
            epoch: Mutex::new((0, Instant::now() - EPOCH_REFRESH * 2)),
        };
        // Fail at startup, not on the first request, when no server answers.
        cache.current_epoch()?;
        Ok(cache)
    }

    fn server_for(&self, key: &str) -> &Mutex<Conn> {
        let digest = Sha256::digest(key.as_bytes());
        let n = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]) as usize;
        &self.servers[n % self.servers.len()]
    }

    fn epoch_key(&self) -> String {
        format!("{}epoch", self.key_prefix)
    }

    /// The epoch every stored key carries. Created on first use.
    fn current_epoch(&self) -> Result<u64, CacheError> {
        {
            let guard = self.epoch.lock().unwrap_or_else(|e| e.into_inner());
            if guard.1.elapsed() < EPOCH_REFRESH {
                return Ok(guard.0);
            }
        }
        let key = self.epoch_key();
        let value = match self.raw_get(&key)? {
            Some(bytes) => std::str::from_utf8(&bytes)
                .ok()
                .and_then(|text| text.trim().parse::<u64>().ok()),
            None => None,
        };
        let value = match value {
            Some(value) => value,
            None => {
                // `add` so two nodes starting together agree on one value.
                let fresh = now_millis();
                self.raw_add(&key, fresh.to_string().as_bytes())?;
                self.raw_get(&key)?
                    .and_then(|bytes| String::from_utf8(bytes).ok())
                    .and_then(|text| text.trim().parse::<u64>().ok())
                    .unwrap_or(fresh)
            }
        };
        *self.epoch.lock().unwrap_or_else(|e| e.into_inner()) = (value, Instant::now());
        Ok(value)
    }

    /// Everything stored so far is now unreachable.
    fn bump_epoch(&self) -> Result<(), CacheError> {
        let current = self.current_epoch().unwrap_or(0);
        let next = now_millis().max(current.saturating_add(1));
        self.raw_set(&self.epoch_key(), next.to_string().as_bytes(), None)?;
        *self.epoch.lock().unwrap_or_else(|e| e.into_inner()) = (next, Instant::now());
        Ok(())
    }

    fn store_key(&self, key: &str) -> Result<String, CacheError> {
        let epoch = self.current_epoch()?;
        let plain = key
            .bytes()
            .all(|b| b.is_ascii_graphic() && b != b'\\')
            && key.len() + self.key_prefix.len() + 24 <= MAX_KEY;
        let body = if plain {
            key.to_string()
        } else {
            let digest = Sha256::digest(key.as_bytes());
            let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
            format!("h:{hex}")
        };
        Ok(format!("{}{epoch:x}:{body}", self.key_prefix))
    }

    fn raw_get(&self, key: &str) -> Result<Option<Vec<u8>>, CacheError> {
        let mut conn = self
            .server_for(key)
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let key = key.to_string();
        conn.run(move |s| {
            s.get_mut().write_all(format!("get {key}\r\n").as_bytes())?;
            let mut line = String::new();
            s.read_line(&mut line)?;
            if line.starts_with("END") {
                return Ok(None);
            }
            // VALUE <key> <flags> <bytes>
            let len: usize = line
                .split_whitespace()
                .nth(3)
                .and_then(|n| n.parse().ok())
                .ok_or_else(|| std::io::Error::other(format!("bad reply `{}`", line.trim())))?;
            let mut data = vec![0u8; len + 2];
            s.read_exact(&mut data)?;
            data.truncate(len);
            let mut end = String::new();
            s.read_line(&mut end)?;
            Ok(Some(data))
        })
    }

    fn raw_set(&self, key: &str, value: &[u8], ttl: Option<Duration>) -> Result<(), CacheError> {
        self.raw_store("set", key, value, ttl).map(|_| ())
    }

    fn raw_add(&self, key: &str, value: &[u8]) -> Result<bool, CacheError> {
        self.raw_store("add", key, value, None)
    }

    fn raw_store(
        &self,
        verb: &str,
        key: &str,
        value: &[u8],
        ttl: Option<Duration>,
    ) -> Result<bool, CacheError> {
        let exptime = ttl.map_or(0, |d| d.as_secs().max(1));
        let mut conn = self
            .server_for(key)
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let head = format!("{verb} {key} 0 {exptime} {}\r\n", value.len());
        conn.run(|s| {
            {
                let w = s.get_mut();
                w.write_all(head.as_bytes())?;
                w.write_all(value)?;
                w.write_all(b"\r\n")?;
            }
            let mut line = String::new();
            s.read_line(&mut line)?;
            match line.trim() {
                "STORED" => Ok(true),
                "NOT_STORED" => Ok(false),
                other => Err(std::io::Error::other(format!("store reply `{other}`"))),
            }
        })
    }

    fn raw_delete(&self, key: &str) -> Result<bool, CacheError> {
        let mut conn = self
            .server_for(key)
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let head = format!("delete {key}\r\n");
        conn.run(|s| {
            s.get_mut().write_all(head.as_bytes())?;
            let mut line = String::new();
            s.read_line(&mut line)?;
            match line.trim() {
                "DELETED" => Ok(true),
                "NOT_FOUND" => Ok(false),
                other => Err(std::io::Error::other(format!("delete reply `{other}`"))),
            }
        })
    }
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(1)
}

impl CacheStore for MemcachedCache {
    fn get(&self, key: &str) -> Result<Option<CacheValue>, CacheError> {
        let stored = self.store_key(key)?;
        Ok(self.raw_get(&stored)?.map(CacheValue::new))
    }

    fn set(&self, key: &str, value: CacheValue, ttl: Option<Duration>) -> Result<(), CacheError> {
        if value.len() > self.max_value_bytes {
            return Err(CacheError::ValueTooLarge {
                max: self.max_value_bytes,
            });
        }
        let stored = self.store_key(key)?;
        self.raw_set(&stored, value.as_slice(), ttl)
    }

    fn delete(&self, key: &str) -> Result<bool, CacheError> {
        let stored = self.store_key(key)?;
        self.raw_delete(&stored)
    }

    fn delete_prefix(&self, _prefix: &str) -> Result<usize, CacheError> {
        // See the module docs: the whole key space of this prefix moves to a new epoch.
        self.bump_epoch()?;
        Ok(0)
    }

    fn clear(&self) -> Result<(), CacheError> {
        self.bump_epoch()
    }

    fn len(&self) -> usize {
        // Memcached cannot enumerate its keys.
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// A just-enough memcached for the commands the client sends.
    fn fake_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let store: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::default();
        std::thread::spawn(move || {
            for conn in listener.incoming().flatten() {
                let store = Arc::clone(&store);
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(conn.try_clone().unwrap());
                    let mut out = conn;
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 {
                            return;
                        }
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        match parts.as_slice() {
                            ["get", key] => {
                                let hit = store.lock().unwrap().get(*key).cloned();
                                match hit {
                                    Some(v) => {
                                        let _ = write!(out, "VALUE {key} 0 {}\r\n", v.len());
                                        let _ = out.write_all(&v);
                                        let _ = out.write_all(b"\r\nEND\r\n");
                                    }
                                    None => {
                                        let _ = out.write_all(b"END\r\n");
                                    }
                                }
                            }
                            [verb @ ("set" | "add"), key, _flags, _exp, len] => {
                                let len: usize = len.parse().unwrap();
                                let mut data = vec![0u8; len + 2];
                                reader.read_exact(&mut data).unwrap();
                                data.truncate(len);
                                let mut s = store.lock().unwrap();
                                if *verb == "add" && s.contains_key(*key) {
                                    let _ = out.write_all(b"NOT_STORED\r\n");
                                } else {
                                    s.insert(key.to_string(), data);
                                    let _ = out.write_all(b"STORED\r\n");
                                }
                            }
                            ["delete", key] => {
                                let had = store.lock().unwrap().remove(*key).is_some();
                                let _ = out.write_all(if had {
                                    b"DELETED\r\n".as_slice()
                                } else {
                                    b"NOT_FOUND\r\n".as_slice()
                                });
                            }
                            _ => {
                                let _ = out.write_all(b"ERROR\r\n");
                            }
                        }
                    }
                });
            }
        });
        addr
    }

    fn client() -> MemcachedCache {
        MemcachedCache::connect(
            &MemcachedCacheConfig {
                servers: vec![fake_server()],
                key_prefix: "t:".into(),
            },
            1024,
        )
        .unwrap()
    }

    #[test]
    fn set_get_delete_round_trip() {
        let c = client();
        c.set("ns::a", CacheValue::new("hello"), Some(Duration::from_secs(5)))
            .unwrap();
        assert_eq!(c.get("ns::a").unwrap().unwrap().as_slice(), b"hello");
        assert!(c.delete("ns::a").unwrap());
        assert!(c.get("ns::a").unwrap().is_none());
        assert!(!c.delete("ns::a").unwrap());
    }

    #[test]
    fn odd_and_long_keys_are_hashed() {
        let c = client();
        let long = format!("ns::{}", "k".repeat(400));
        c.set(&long, CacheValue::new("x"), None).unwrap();
        c.set("ns::has space\nand newline", CacheValue::new("y"), None)
            .unwrap();
        assert_eq!(c.get(&long).unwrap().unwrap().as_slice(), b"x");
        assert_eq!(
            c.get("ns::has space\nand newline").unwrap().unwrap().as_slice(),
            b"y"
        );
    }

    #[test]
    fn binary_values_survive() {
        let c = client();
        let bytes: Vec<u8> = (0..=255u8).collect();
        c.set("ns::bin", CacheValue::new(bytes.clone()), None).unwrap();
        assert_eq!(c.get("ns::bin").unwrap().unwrap().bytes, bytes);
    }

    #[test]
    fn prefix_delete_moves_to_a_new_epoch() {
        let c = client();
        c.set("ns::a", CacheValue::new("1"), None).unwrap();
        c.delete_prefix("ns::").unwrap();
        assert!(c.get("ns::a").unwrap().is_none());
        c.set("ns::a", CacheValue::new("2"), None).unwrap();
        assert_eq!(c.get("ns::a").unwrap().unwrap().as_slice(), b"2");
    }

    #[test]
    fn oversized_values_are_refused() {
        let c = client();
        assert!(matches!(
            c.set("ns::big", CacheValue::new(vec![0u8; 2048]), None),
            Err(CacheError::ValueTooLarge { .. })
        ));
    }
}
