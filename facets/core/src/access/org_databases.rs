//! Mantle warm-set for the global `org_databases` registry.

use serde::Deserialize;
use surrealdb::types::SurrealValue;

use crate::cache::Cache;
use crate::state::AppState;

#[derive(Debug, Deserialize, SurrealValue)]
struct DbNameRow {
    db_name: String,
}

/// Organization database names from Mantle, filling from core on miss.
pub async fn list_cached(state: &AppState) -> Result<Vec<String>, surrealdb::Error> {
    if let Ok(Some(names)) = state.cache.warm_get_org_databases() {
        return Ok(names);
    }
    refresh(state).await
}

/// Reload `org_databases` from core and replace the warm entry.
pub async fn refresh(state: &AppState) -> Result<Vec<String>, surrealdb::Error> {
    let core = state.core().await?;
    let mut response = core
        .query("SELECT db_name FROM org_databases;")
        .await?
        .check()?;
    let rows: Vec<DbNameRow> = response.take(0)?;
    let names: Vec<String> = rows.into_iter().map(|row| row.db_name).collect();
    let _ = state.cache.warm_set_org_databases(&names);
    Ok(names)
}

/// Seed Mantle at process start (best-effort).
pub async fn warm_at_boot(state: &AppState) {
    if let Err(error) = refresh(state).await {
        log::warn!("org_databases warm at boot: {error}");
    }
}

pub fn invalidate(cache: &Cache) {
    let _ = cache.warm_invalidate_org_databases();
}
