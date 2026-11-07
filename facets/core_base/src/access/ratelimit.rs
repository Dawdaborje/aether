use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use moka::sync::Cache;

const WINDOW: Duration = Duration::from_secs(60);
const MAX_TRACKED_ADDRESSES: u64 = 100_000;

/// Per-client-address limit over a fixed one-minute window.
///
/// Used for anonymous traffic: how many requests an address may make, and how
/// many new visitor identities it may create. Counters live in memory, so the
/// budget is per server process and resets on restart.
#[derive(Clone)]
pub struct RateLimiter {
    windows: Cache<IpAddr, Arc<AtomicU32>>,
    max_per_window: u32,
}

impl RateLimiter {
    pub fn new(max_per_minute: u32) -> Self {
        Self {
            windows: Cache::builder()
                .max_capacity(MAX_TRACKED_ADDRESSES)
                .time_to_live(WINDOW)
                .build(),
            max_per_window: max_per_minute,
        }
    }

    /// Count one event for `ip`; `false` once the window's budget is spent.
    /// Events with no known address share one bucket.
    pub fn allow(&self, ip: Option<IpAddr>) -> bool {
        let key = ip.unwrap_or(IpAddr::from([0, 0, 0, 0]));
        let counter = self.windows.get_with(key, || Arc::new(AtomicU32::new(0)));
        counter.fetch_add(1, Ordering::Relaxed) < self.max_per_window
    }
}

/// Failed attempts per name (a username), over a fixed one-minute window. Unlike [`RateLimiter`] it
/// counts only what the caller records, so a name is blocked by its failures and never by successes.
#[derive(Clone)]
pub struct FailureLimiter {
    windows: Cache<String, Arc<AtomicU32>>,
    max_per_window: u32,
}

impl FailureLimiter {
    pub fn new(max_failures_per_minute: u32) -> Self {
        Self {
            windows: Cache::builder()
                .max_capacity(MAX_TRACKED_ADDRESSES)
                .time_to_live(WINDOW)
                .build(),
            max_per_window: max_failures_per_minute,
        }
    }

    /// Whether `name` has used up its failures for this window.
    pub fn blocked(&self, name: &str) -> bool {
        self.windows
            .get(&Self::key(name))
            .is_some_and(|counter| counter.load(Ordering::Relaxed) >= self.max_per_window)
    }

    /// Count one failure for `name`.
    pub fn record(&self, name: &str) {
        let counter = self.windows.get_with(Self::key(name), || Arc::new(AtomicU32::new(0)));
        counter.fetch_add(1, Ordering::Relaxed);
    }

    /// Names differing in case or surrounding spaces share one budget.
    fn key(name: &str) -> String {
        name.trim().to_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spends_a_per_address_budget() {
        let limiter = RateLimiter::new(2);
        let one: IpAddr = [203, 0, 113, 1].into();
        let two: IpAddr = [203, 0, 113, 2].into();
        assert!(limiter.allow(Some(one)));
        assert!(limiter.allow(Some(one)));
        assert!(!limiter.allow(Some(one)));
        assert!(
            limiter.allow(Some(two)),
            "other addresses have their own budget"
        );
    }

    #[test]
    fn a_zero_budget_allows_nothing() {
        assert!(!RateLimiter::new(0).allow(None));
    }

    #[test]
    fn unknown_addresses_share_one_bucket() {
        let limiter = RateLimiter::new(1);
        assert!(limiter.allow(None));
        assert!(!limiter.allow(None));
    }

    #[test]
    fn a_name_is_blocked_by_its_failures_only() {
        let limiter = FailureLimiter::new(2);
        assert!(!limiter.blocked("Ada"));
        limiter.record("ada");
        assert!(!limiter.blocked("ada"));
        limiter.record(" ADA ");
        assert!(limiter.blocked("Ada"), "case and spaces do not give a fresh budget");
        assert!(!limiter.blocked("bob"));
    }
}
