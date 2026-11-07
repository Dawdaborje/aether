pub use aether_core_base::notifications::service::*;

use super::store;

/// How often expired and old notifications are removed.
const CLEANUP_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// Remove expired notifications (and, with `[notifications] retention_days`, old ones)
/// from every organization, now and then every hour. Returns the task so shutdown can
/// stop it.
pub fn spawn_cleanup(state: &crate::state::AppState) -> tokio::task::JoinHandle<()> {
    let state = state.clone();
    tokio::spawn(async move {
        loop {
            match cleanup(&state).await {
                Ok(0) => {}
                Ok(removed) => log::info!("Notification cleanup removed {removed} notification(s)"),
                Err(error) => log::warn!("Notification cleanup failed: {error}"),
            }
            tokio::time::sleep(CLEANUP_INTERVAL).await;
        }
    })
}

async fn cleanup(state: &crate::state::AppState) -> Result<u64, surrealdb::Error> {
    let organizations = crate::access::org_databases::list_cached(state).await?;
    let days = state.config.notifications.retention_days;
    let mut removed = 0;
    for db_name in organizations {
        let db = state.org(&db_name).await?;
        removed += store::purge(&db, days).await?;
    }
    Ok(removed)
}
