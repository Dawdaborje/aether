//! Org `installed_plugins` via Mantle warm-set.

use serde::Deserialize;
use surrealdb::types::{Datetime, SurrealValue};

use crate::cache::{Cache, InstalledPluginWarm};
use crate::state::Db;

#[derive(Debug, Clone, Deserialize, SurrealValue)]
struct InstalledRow {
    plugin_name: String,
    version: String,
    is_enabled: bool,
    /// Selected so SurrealDB allows `ORDER BY installed_at`.
    #[allow(dead_code)]
    installed_at: Option<Datetime>,
}

/// All installed plugins for an organization (enabled and disabled), install order.
pub async fn list_cached(
    cache: &Cache,
    org_name: &str,
    org: &Db,
) -> Result<Vec<InstalledPluginWarm>, surrealdb::Error> {
    if let Ok(Some(rows)) = cache.warm_get_installed_plugins(org_name) {
        return Ok(rows);
    }
    let mut response = org
        .query(
            "SELECT plugin_name, version, is_enabled, installed_at FROM installed_plugins ORDER BY installed_at;",
        )
        .await?
        .check()?;
    let rows: Vec<InstalledRow> = response.take(0)?;
    let warm: Vec<InstalledPluginWarm> = rows
        .into_iter()
        .map(|row| InstalledPluginWarm {
            plugin_name: row.plugin_name,
            version: row.version,
            is_enabled: row.is_enabled,
        })
        .collect();
    let _ = cache.warm_set_installed_plugins(org_name, &warm);
    Ok(warm)
}

/// Enabled plugins as `(name, version)` pairs in install order.
pub fn enabled_pairs(rows: &[InstalledPluginWarm]) -> Vec<(String, String)> {
    rows.iter()
        .filter(|row| row.is_enabled)
        .map(|row| (row.plugin_name.clone(), row.version.clone()))
        .collect()
}

/// Drop the warm entry after install / upgrade / enable / disable.
pub fn invalidate(cache: &Cache, org_name: &str) {
    let _ = cache.warm_invalidate_installed_plugins(org_name);
    let _ = cache.warm_invalidate_apps_launcher(org_name);
}
