//! The append-only audit trail, written to the organization's database.
//!
//! Writes are synchronous and their errors propagate: if a row cannot be
//! recorded the request fails, so there is never a successful access without
//! a trace.

use std::time::Duration;

use rand::RngExt;
use surrealdb::{Surreal, engine::remote::ws::Client, types::SurrealValue};
use thiserror::Error;

const MAX_USER_AGENT_CHARS: usize = 512;

#[derive(Debug, Error)]
pub enum AuditError {
    #[error("audit write failed: {0}")]
    Database(#[from] surrealdb::Error),
}

/// Who performed an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    /// A logged-in user (`users:…`).
    User(String),
    /// An anonymous visitor (`visitors:…`).
    Visitor(String),
    /// No identity yet: the request was refused before a visitor was issued.
    Anonymous,
    /// The kernel itself, running a background job or a scheduled task (`system:scheduler`).
    System(String),
}

impl Actor {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::User(_) => "user",
            Self::Visitor(_) => "visitor",
            Self::Anonymous => "anonymous",
            Self::System(_) => "system",
        }
    }

    pub fn id(&self) -> Option<&str> {
        match self {
            Self::User(id) | Self::Visitor(id) | Self::System(id) => Some(id),
            Self::Anonymous => None,
        }
    }
}

/// Everything an audit row records about the request that caused it.
#[derive(Debug, Clone)]
pub struct AuditContext {
    pub actor: Actor,
    /// Ties together the page visit, plugin call and data accesses of one request.
    pub request_id: String,
    /// Client address after the `[audit] ip` policy.
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

pub fn new_request_id() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn clip_user_agent(raw: &str) -> String {
    raw.chars().take(MAX_USER_AGENT_CHARS).collect()
}

#[derive(Debug, Clone)]
pub struct PageVisit<'a> {
    pub plugin: Option<&'a str>,
    /// The page's route pattern, when a page was matched.
    pub route: Option<&'a str>,
    /// The path the client asked for.
    pub path: &'a str,
    pub method: &'a str,
    pub status: u16,
}

/// Record a page visit. `db` must already be on the organization's database.
pub async fn record_page_visit(
    db: &Surreal<Client>,
    ctx: &AuditContext,
    visit: &PageVisit<'_>,
) -> Result<(), AuditError> {
    db.query(
        r#"
        CREATE page_visits SET
            request_id = $request_id,
            actor_type = $actor_type,
            actor_id = $actor_id,
            plugin = $plugin,
            route = $route,
            path = $path,
            method = $method,
            status = $status,
            ip = $ip,
            user_agent = $user_agent;
        "#,
    )
    .bind(("request_id", ctx.request_id.clone()))
    .bind(("actor_type", ctx.actor.kind().to_string()))
    .bind(("actor_id", ctx.actor.id().map(str::to_string)))
    .bind(("plugin", visit.plugin.map(str::to_string)))
    .bind(("route", visit.route.map(str::to_string)))
    .bind(("path", visit.path.to_string()))
    .bind(("method", visit.method.to_string()))
    .bind(("status", i64::from(visit.status)))
    .bind(("ip", ctx.ip.clone()))
    .bind(("user_agent", ctx.user_agent.clone()))
    .await?
    .check()?;
    Ok(())
}

/// Record a plugin function call. `db` must be on the organization's database.
pub async fn record_plugin_call(
    db: &Surreal<Client>,
    ctx: &AuditContext,
    plugin: &str,
    function: &str,
    status: u16,
) -> Result<(), AuditError> {
    db.query(
        r#"
        CREATE plugin_calls SET
            request_id = $request_id,
            actor_type = $actor_type,
            actor_id = $actor_id,
            plugin = $plugin,
            function_name = $function,
            status = $status,
            ip = $ip,
            user_agent = $user_agent;
        "#,
    )
    .bind(("request_id", ctx.request_id.clone()))
    .bind(("actor_type", ctx.actor.kind().to_string()))
    .bind(("actor_id", ctx.actor.id().map(str::to_string)))
    .bind(("plugin", plugin.to_string()))
    .bind(("function", function.to_string()))
    .bind(("status", i64::from(status)))
    .bind(("ip", ctx.ip.clone()))
    .bind(("user_agent", ctx.user_agent.clone()))
    .await?
    .check()?;
    Ok(())
}

/// What a retention purge removed.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PurgeReport {
    pub organizations: usize,
    pub page_visits: u64,
    pub plugin_calls: u64,
    pub data_access: u64,
    pub visitors: u64,
}

#[derive(Debug, serde::Deserialize, SurrealValue)]
struct OrgDatabaseName {
    db_name: String,
}

#[derive(Debug, serde::Deserialize, SurrealValue)]
struct Counted {
    count: u64,
}

/// Delete audit rows older than `retention_days` from every organization
/// database, along with visitors not seen since then.
pub async fn purge_expired(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    retention_days: u32,
) -> Result<PurgeReport, AuditError> {
    db.use_ns(namespace).await?;
    db.use_db(core_database).await?;
    let mut response = db.query("SELECT db_name FROM org_databases;").await?.check()?;
    let organizations: Vec<OrgDatabaseName> = response.take(0)?;

    let mut report = PurgeReport::default();
    for organization in organizations {
        db.use_db(&organization.db_name).await?;
        let mut response = db
            .query(
                r#"
                LET $cutoff = time::now() - <duration> string::concat($days, 'd');
                SELECT count() AS count FROM page_visits WHERE date_created < $cutoff GROUP ALL;
                SELECT count() AS count FROM plugin_calls WHERE date_created < $cutoff GROUP ALL;
                SELECT count() AS count FROM data_access WHERE date_created < $cutoff GROUP ALL;
                SELECT count() AS count FROM visitors WHERE last_seen < $cutoff GROUP ALL;
                DELETE page_visits WHERE date_created < $cutoff;
                DELETE plugin_calls WHERE date_created < $cutoff;
                DELETE data_access WHERE date_created < $cutoff;
                DELETE visitors WHERE last_seen < $cutoff;
                "#,
            )
            .bind(("days", i64::from(retention_days)))
            .await?
            .check()?;
        let count = |response: &mut surrealdb::IndexedResults, index: usize| -> u64 {
            response
                .take::<Vec<Counted>>(index)
                .ok()
                .and_then(|rows| rows.into_iter().next())
                .map_or(0, |row| row.count)
        };
        report.page_visits += count(&mut response, 1);
        report.plugin_calls += count(&mut response, 2);
        report.data_access += count(&mut response, 3);
        report.visitors += count(&mut response, 4);
        report.organizations += 1;
    }
    Ok(report)
}

/// How often the server purges when `[audit] retention_days` is set.
pub const PURGE_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// Start the background task that applies `[audit] retention_days`, if set.
/// It purges once at start-up and then every [`PURGE_INTERVAL`].
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
            match purge_expired(&db, &namespace, &core_database, days).await {
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
            tokio::time::sleep(PURGE_INTERVAL).await;
        }
    }))
}
