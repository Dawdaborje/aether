//! The parts of access control that need nothing else from the kernel: the audit trail and its
//! hash chain, API tokens and rate limits. Identity and the services are in `aether_core::access`.

pub mod api_token;
pub mod audit;
pub mod chain;
pub mod ratelimit;

/// Answers to rule variables (`$hr.employee`), kept for one request so a rule is not worked out
/// again by every query. Shared by the plugins a request calls.
#[derive(Debug, Clone, Default)]
pub struct RuleCache(
    pub std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, serde_json::Value>>>,
);

impl RuleCache {
    pub fn get(&self, name: &str) -> Option<serde_json::Value> {
        self.0.lock().ok().and_then(|map| map.get(name).cloned())
    }

    pub fn set(&self, name: &str, value: serde_json::Value) {
        if let Ok(mut map) = self.0.lock() {
            map.insert(name.to_string(), value);
        }
    }
}
