//! Mantle eviction pressure: kind dignity, heat, Tier A pins.

use super::heat::HeatLedger;
use super::labels::{CacheKind, CacheLabel};

/// Tier A warm-set keys that must not lose to plugin `list:` thrash.
pub fn is_tier_a_pinned(label: &CacheLabel, key: &str) -> bool {
    if !label.is_kernel {
        return false;
    }
    matches!(
        key,
        "org_databases" | "cap_catalog" | "settings:catalog" | "installed_plugins"
    ) || key.starts_with("session:")
        || key.starts_with("stamp:")
        || (key.starts_with("user:") && (key.ends_with(":memberships") || key.ends_with(":roles")))
        || key.starts_with("setting:g:")
        || key.starts_with("setting:r:")
}

/// Whether reclaim may drop this key under elastic pressure. Kernel keys and `meta:` entries
/// stay; rows go only after every list/aggregate (see [`CacheKind::dignity`] ordering).
pub fn is_reclaim_candidate(label: &CacheLabel, key: &str) -> bool {
    if is_tier_a_pinned(label, key) {
        return false;
    }
    matches!(
        label.kind,
        CacheKind::List | CacheKind::Other | CacheKind::Agg | CacheKind::Row
    )
}

/// Per-tick reclaim budget so elastic trim is gradual (not a thundering dump).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReclaimBudget {
    pub max_bytes: u64,
    pub max_keys: usize,
}

/// Cap one `maintain()` pass: ~5% of `target` (min 1KiB), never more than overage, ≤32 keys.
pub fn reclaim_budget(used: u64, target: u64) -> ReclaimBudget {
    let overage = used.saturating_sub(target);
    if overage == 0 {
        return ReclaimBudget {
            max_bytes: 0,
            max_keys: 0,
        };
    }
    let slice = (target / 20).max(1024).min(overage);
    ReclaimBudget {
        max_bytes: slice,
        max_keys: 32,
    }
}

/// Bytes an entry occupies: key + value + fixed bookkeeping. This is both the Moka weight and
/// what the stats count, so the byte budget, the elastic target and the counters agree.
pub fn entry_cost(fq_key: &str, value_len: usize) -> u64 {
    (fq_key.len() as u64)
        .saturating_add(value_len as u64)
        .saturating_add(64)
}

/// Moka weight of an entry. Plain bytes: Moka's admission (TinyLFU) stays honest about
/// capacity, and the *which entry goes first* decision lives in `maintain()`
/// (dignity, heat, adaptive lists). Tier A pins weigh `1` so they are effectively never
/// evicted by size.
pub fn entry_weight(fq_key: &str, value_len: usize) -> u32 {
    let label = CacheLabel::parse(fq_key);
    let key = fq_key.split_once("::").map(|(_, k)| k).unwrap_or(fq_key);
    if is_tier_a_pinned(&label, key) {
        return 1;
    }
    // Keep unpinned entries strictly heavier than a Tier A pin.
    entry_cost(fq_key, value_len).clamp(2, u32::MAX as u64) as u32
}

/// Heat of a key's plugin/model. Used to order reclaim: colder goes first.
pub fn heat_score(label: &CacheLabel, heat: &HeatLedger) -> f64 {
    let Some(org) = label.org.as_deref().filter(|o| !o.is_empty()) else {
        return 0.0;
    };
    let Some(plugin) = label.plugin.as_deref() else {
        return 0.0;
    };
    heat.priority(org, plugin, label.model.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weight_is_bytes_not_dignity() {
        let list = entry_weight("plugin:acme:crm::list:contact:abc", 100);
        let row = entry_weight("plugin:acme:crm::row:contact:abc", 100);
        // Same value, same cost: only the one extra key byte of `list` differs.
        assert_eq!(list, row + 1);
        assert!(list as u64 >= 100);
    }

    #[test]
    fn tier_a_is_pinned() {
        assert_eq!(entry_weight("kernel::session:deadbeef", 500), 1);
        assert_eq!(entry_weight("kernel::org_databases", 200), 1);
        assert_eq!(entry_weight("kernel:acme::user:users:1:roles", 80), 1);
        assert_eq!(entry_weight("kernel:acme::installed_plugins", 400), 1);
        assert_eq!(entry_weight("kernel:acme::stamp:contact", 20), 1);
    }

    #[test]
    fn tier_b_kernel_pages_not_pinned() {
        assert!(entry_weight("kernel::plugin:notes@1.0.0:pages", 2000) > 1);
    }

    #[test]
    fn rows_are_reclaimable_after_lists_but_meta_is_not() {
        let row = CacheLabel::parse("plugin:acme:crm::row:contact:1");
        let meta = CacheLabel::parse("plugin:acme:crm::meta:schema");
        assert!(is_reclaim_candidate(&row, "row:contact:1"));
        assert!(!is_reclaim_candidate(&meta, "meta:schema"));
    }

    #[test]
    fn reclaim_budget_is_gradual() {
        let b = reclaim_budget(10_000, 8_000);
        assert_eq!(b.max_bytes, 1024);
        assert_eq!(b.max_keys, 32);
        assert_eq!(reclaim_budget(8_000, 8_000).max_bytes, 0);
    }

    #[test]
    fn dignity_order() {
        assert!(CacheKind::Meta.dignity() > CacheKind::Row.dignity());
        assert!(CacheKind::Row.dignity() > CacheKind::Agg.dignity());
        assert!(CacheKind::Agg.dignity() > CacheKind::List.dignity());
    }
}
