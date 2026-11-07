//! A limit on the outgoing calls a plugin makes (`http::request` and `bridge::call`).
//!
//! Each call is already limited in size and time, so one plugin could still make many of them,
//! or leave many waiting, and tie up the server's network and a bridge's account. Two limits stop
//! that, per plugin and organization: calls started in the current minute, and calls waiting for
//! an answer. Both are set under `[plugin_runtime]` (`outbound_calls_per_minute`,
//! `outbound_max_concurrent`); 0 turns a limit off.
//!
//! The counters live in memory, so the budget is per server process and resets on restart.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::context::PluginHostContext;
use super::error::HostError;

const WINDOW: Duration = Duration::from_secs(60);
/// Past this many tracked plugins, idle entries are dropped.
const PRUNE_ABOVE: usize = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub per_minute: u32,
    pub max_concurrent: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self { per_minute: 600, max_concurrent: 16 }
    }
}

struct Entry {
    window_start: Instant,
    started: u32,
    waiting: u32,
}

#[derive(Clone)]
pub struct Outbound {
    limits: Limits,
    entries: Arc<Mutex<HashMap<(String, String), Entry>>>,
}

/// Held while a call waits for its answer; frees the slot when dropped.
pub struct Permit {
    key: (String, String),
    entries: Arc<Mutex<HashMap<(String, String), Entry>>>,
}

impl Drop for Permit {
    fn drop(&mut self) {
        if let Ok(mut entries) = self.entries.lock()
            && let Some(entry) = entries.get_mut(&self.key)
        {
            entry.waiting = entry.waiting.saturating_sub(1);
        }
    }
}

impl Outbound {
    pub fn new(limits: Limits) -> Self {
        Self { limits, entries: Arc::default() }
    }

    /// Count one outgoing call by `plugin` in `organization`, or say why it may not start.
    pub fn acquire(&self, plugin: &str, organization: &str, now: Instant) -> Result<Permit, HostError> {
        let key = (plugin.to_string(), organization.to_string());
        let mut entries = self.entries.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if entries.len() > PRUNE_ABOVE {
            entries.retain(|_, entry| entry.waiting > 0 || now.duration_since(entry.window_start) < WINDOW);
        }
        let entry = entries
            .entry(key.clone())
            .or_insert(Entry { window_start: now, started: 0, waiting: 0 });
        if now.duration_since(entry.window_start) >= WINDOW {
            entry.window_start = now;
            entry.started = 0;
        }
        let Limits { per_minute, max_concurrent } = self.limits;
        if per_minute > 0 && entry.started >= per_minute {
            return Err(HostError::Message(format!(
                "rate limit: this plugin has made {per_minute} outgoing calls in the last minute; try again shortly"
            )));
        }
        if max_concurrent > 0 && entry.waiting >= max_concurrent {
            return Err(HostError::Message(format!(
                "rate limit: this plugin already has {max_concurrent} outgoing calls waiting for an answer"
            )));
        }
        entry.started += 1;
        entry.waiting += 1;
        Ok(Permit { key, entries: self.entries.clone() })
    }
}

static PROCESS: OnceLock<Mutex<Outbound>> = OnceLock::new();

fn process() -> &'static Mutex<Outbound> {
    PROCESS.get_or_init(|| Mutex::new(Outbound::new(Limits::default())))
}

/// Set the limits for this process. Counters already running are kept only if the limits did not change.
pub fn configure(limits: Limits) {
    let mut outbound = process().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if outbound.limits != limits {
        *outbound = Outbound::new(limits);
    }
}

/// Count an outgoing call by the plugin running in `ctx`. Keep the permit until the answer is back.
pub fn acquire(ctx: &PluginHostContext) -> Result<Permit, HostError> {
    let outbound = process().lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    outbound.acquire(&ctx.plugin_name, &ctx.database, Instant::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits(per_minute: u32, max_concurrent: u32) -> Outbound {
        Outbound::new(Limits { per_minute, max_concurrent })
    }

    #[test]
    fn spends_a_per_minute_budget_then_starts_again() {
        let outbound = limits(2, 0);
        let start = Instant::now();
        drop(outbound.acquire("p", "org", start).unwrap());
        drop(outbound.acquire("p", "org", start).unwrap());
        let refused = outbound.acquire("p", "org", start);
        assert!(matches!(&refused, Err(HostError::Message(m)) if m.contains("rate limit")));
        assert!(outbound.acquire("p", "org", start + WINDOW).is_ok());
    }

    #[test]
    fn limits_calls_waiting_and_frees_a_slot_when_one_finishes() {
        let outbound = limits(0, 2);
        let now = Instant::now();
        let first = outbound.acquire("p", "org", now).unwrap();
        let _second = outbound.acquire("p", "org", now).unwrap();
        assert!(outbound.acquire("p", "org", now).is_err());
        drop(first);
        assert!(outbound.acquire("p", "org", now).is_ok());
    }

    #[test]
    fn each_plugin_and_organization_has_its_own_budget() {
        let outbound = limits(1, 1);
        let now = Instant::now();
        let _a = outbound.acquire("p", "org_a", now).unwrap();
        assert!(outbound.acquire("p", "org_b", now).is_ok());
        assert!(outbound.acquire("q", "org_a", now).is_ok());
        assert!(outbound.acquire("p", "org_a", now).is_err());
    }

    #[test]
    fn zero_turns_a_limit_off() {
        let outbound = limits(0, 0);
        let now = Instant::now();
        let held: Vec<_> = (0..100).map(|_| outbound.acquire("p", "org", now).unwrap()).collect();
        assert_eq!(held.len(), 100);
    }
}
