//! The background task that applies `[audit] retention_days`.

use super::audit;

/// Start the background task that applies `[audit] retention_days`, if set.
/// It purges once at start-up and then every [`audit::PURGE_INTERVAL`].
///
/// Returns the task handle so shutdown can stop it; `None` when no retention
/// is configured.
pub fn spawn_retention_task(state: &crate::state::AppState) -> Option<tokio::task::JoinHandle<()>> {
    let days = state.config.audit.retention_days?;
    let db = state.fresh_session();
    let namespace = state.namespace.clone();
    let core_database = state.core_database.clone();
    Some(tokio::spawn(async move {
        loop {
            match audit::purge_expired(&db, &namespace, &core_database, days).await {
                Ok(report) => log::info!(
                    "Audit retention ({days} days): removed {} page visit(s), {} plugin call(s), {} data access row(s), {} visitor(s) across {} organization(s)",
                    report.page_visits,
                    report.plugin_calls,
                    report.data_access,
                    report.visitors,
                    report.organizations
                ),
                Err(error) => log::error!("Audit retention purge failed: {error}"),
            }
            tokio::time::sleep(audit::PURGE_INTERVAL).await;
        }
    }))
}
