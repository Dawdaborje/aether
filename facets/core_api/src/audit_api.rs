//! Desk HTTP API for the audit log viewer.
//!
//! * `GET  /api/ui/organizations/{db}/audit?table=&…` one page of an audit table, filtered.
//! * `GET  /api/ui/organizations/{db}/audit/chain` the chain's heads and the last verification.
//! * `POST /api/ui/organizations/{db}/audit/verify` check every chain now.
//!
//! The tables are written by the kernel only and this API only reads them (verifying writes
//! one `audit_verifications` row, as the daily check does). The table name comes from a fixed
//! list, never from the request text; every value is bound.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use surrealdb::types::SurrealValue;

use aether_core::application::settings_api::require_org_developer;
use aether_core::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/ui/organizations/{db}/audit", get(list))
        .route("/api/ui/organizations/{db}/audit/chain", get(chain))
        .route("/api/ui/organizations/{db}/audit/verify", post(verify))
}

/// One of the three audit tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Table {
    DataAccess,
    PluginCalls,
    PageVisits,
}

impl Table {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "data_access" => Some(Self::DataAccess),
            "plugin_calls" => Some(Self::PluginCalls),
            "page_visits" => Some(Self::PageVisits),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::DataAccess => "data_access",
            Self::PluginCalls => "plugin_calls",
            Self::PageVisits => "page_visits",
        }
    }

    /// The columns to return (the same names the table has, so the page can show them as they are).
    fn columns(self) -> &'static str {
        match self {
            Self::DataAccess => {
                "request_id, actor_type, actor_id, plugin, function_name, model, table_name, operation, \
                 record_ids.map(|$r| <string> $r) AS record_ids, record_count, statement, ip, chain_seq"
            }
            Self::PluginCalls => {
                "request_id, actor_type, actor_id, plugin, function_name, status, ip, user_agent, chain_seq"
            }
            Self::PageVisits => {
                "request_id, actor_type, actor_id, plugin, route, path, method, status, ip, user_agent, chain_seq"
            }
        }
    }

    /// What the free-text box searches.
    fn haystack(self) -> &'static str {
        match self {
            Self::DataAccess => {
                "string::concat(plugin, ' ', function_name, ' ', model ?? '', ' ', table_name ?? '', ' ', statement ?? '')"
            }
            Self::PluginCalls => "string::concat(plugin, ' ', function_name)",
            Self::PageVisits => {
                "string::concat(plugin ?? '', ' ', route ?? '', ' ', path, ' ', method)"
            }
        }
    }

    /// The table-specific filters, on top of the common ones.
    fn extra_filter(self) -> &'static str {
        match self {
            Self::DataAccess => " AND ($operation = NONE OR operation = $operation)",
            Self::PluginCalls | Self::PageVisits => {
                " AND ($status = NONE OR status = $status) AND ($failed = NONE OR status >= 400)"
            }
        }
    }
}

/// The `WHERE` clause of the list and of its total.
pub fn filter_sql(table: Table) -> String {
    format!(
        "($actor = NONE OR actor_id = $actor OR actor_type = $actor) \
         AND ($request = NONE OR request_id = $request) \
         AND ($plugin = NONE OR plugin = $plugin) \
         AND ($from = NONE OR date_created >= <datetime> $from) \
         AND ($until = NONE OR date_created < <datetime> $until) \
         AND ($q = NONE OR string::contains(string::lowercase({haystack}), $q)){extra}",
        haystack = table.haystack(),
        extra = table.extra_filter()
    )
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    table: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
    /// A user id, a visitor id, or `user` / `visitor` / `anonymous`.
    actor: Option<String>,
    plugin: Option<String>,
    /// Every row of one request, across the tables (the `request_id` shared by its rows).
    request: Option<String>,
    /// `read`, `create`, `update`, `delete` or `raw` (data access only).
    operation: Option<String>,
    /// Exact HTTP-style status (calls and visits only).
    status: Option<i64>,
    /// Only refused or failed requests: status 400 and up (calls and visits only).
    failed: Option<bool>,
    /// RFC 3339 times.
    from: Option<String>,
    until: Option<String>,
    /// Case-insensitive text.
    q: Option<String>,
}

fn failure(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn database_error(context: &str, error: impl std::fmt::Display) -> Response {
    log::error!("audit {context}: {error}");
    failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
}

async fn guard(state: &AppState, headers: &HeaderMap, db: &str) -> Result<aether_core::state::Db, Response> {
    require_org_developer(state.databases(), headers, db)
        .await
        .map_err(|(status, body)| (status, body).into_response())?;
    let core = state.core().await.map_err(|e| database_error("core", e))?;
    let mut response = core
        .query("SELECT db_name FROM org_databases WHERE db_name = $org LIMIT 1;")
        .bind(("org", db.to_string()))
        .await
        .and_then(|r| r.check())
        .map_err(|e| database_error("org lookup", e))?;
    let found: Option<Value> = response.take(0).map_err(|e| database_error("org lookup", e))?;
    if found.is_none() {
        return Err(failure(StatusCode::NOT_FOUND, "organization not found"));
    }
    state.org(db).await.map_err(|e| database_error("open org", e))
}

/// A time bound as RFC 3339 text, or an error message when it is not one.
fn instant(text: &Option<String>, name: &str) -> Result<Option<String>, String> {
    match text.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        None => Ok(None),
        Some(t) => chrono::DateTime::parse_from_rfc3339(t)
            .map(|d| Some(d.with_timezone(&chrono::Utc).to_rfc3339()))
            .map_err(|_| format!("`{name}` must be an RFC 3339 time, like 2026-11-02T09:00:00Z")),
    }
}

fn clean(text: &Option<String>) -> Option<String> {
    text.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(str::to_string)
}

/// The page query and its total, for one table.
pub fn list_sql(table: Table) -> String {
    let filter = filter_sql(table);
    format!(
        "SELECT <string> record::id(id) AS id, <string> date_created AS date_created, {columns} \
         FROM {name} WHERE {filter} ORDER BY date_created DESC LIMIT $limit START $offset; \
         SELECT count() AS n FROM {name} WHERE {filter} GROUP ALL;",
        columns = table.columns(),
        name = table.name(),
    )
}

#[derive(Debug, Deserialize, SurrealValue)]
struct CountRow {
    n: i64,
}

async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(db): Path<String>,
    Query(query): Query<ListQuery>,
) -> Response {
    let org = match guard(&state, &headers, &db).await {
        Ok(org) => org,
        Err(response) => return response,
    };
    let Some(table) = Table::parse(query.table.as_deref().unwrap_or("data_access")) else {
        return failure(StatusCode::BAD_REQUEST, "`table` must be data_access, plugin_calls or page_visits");
    };
    let from = match instant(&query.from, "from") {
        Ok(v) => v,
        Err(message) => return failure(StatusCode::BAD_REQUEST, &message),
    };
    let until = match instant(&query.until, "until") {
        Ok(v) => v,
        Err(message) => return failure(StatusCode::BAD_REQUEST, &message),
    };
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);
    let sql = list_sql(table);
    let mut response = match org
        .query(sql)
        .bind(("limit", limit))
        .bind(("offset", offset))
        .bind(("actor", clean(&query.actor)))
        .bind(("plugin", clean(&query.plugin)))
        .bind(("request", clean(&query.request)))
        .bind(("operation", clean(&query.operation)))
        .bind(("status", query.status))
        .bind(("failed", query.failed.filter(|f| *f)))
        .bind(("from", from))
        .bind(("until", until))
        .bind(("q", clean(&query.q).map(|q| q.to_lowercase())))
        .await
        .and_then(|r| r.check())
    {
        Ok(response) => response,
        Err(error) => return database_error("list", error),
    };
    let rows: Vec<Value> = match response.take(0) {
        Ok(rows) => rows,
        Err(error) => return database_error("list", error),
    };
    let total = match response.take::<Vec<CountRow>>(1) {
        Ok(rows) => rows.first().map_or(0, |row| row.n),
        Err(error) => return database_error("count", error),
    };
    Json(json!({
        "table": table.name(),
        "total": total,
        "offset": offset,
        "limit": limit,
        "rows": rows,
    }))
    .into_response()
}

/// The chain heads and the latest verification. Either may be missing in an organization the
/// sealer has not reached yet.
async fn chain(State(state): State<AppState>, headers: HeaderMap, Path(db): Path<String>) -> Response {
    let org = match guard(&state, &headers, &db).await {
        Ok(org) => org,
        Err(response) => return response,
    };
    let heads: Vec<Value> = org
        .query("SELECT chain, seq, hash, base_seq FROM audit_chain_head ORDER BY chain;")
        .await
        .and_then(|r| r.check())
        .and_then(|mut r| r.take(0))
        .unwrap_or_default();
    let last: Option<Value> = org
        .query("SELECT ok, <string> at AS at, report FROM audit_verifications ORDER BY at DESC LIMIT 1;")
        .await
        .and_then(|r| r.check())
        .and_then(|mut r| r.take::<Vec<Value>>(0))
        .ok()
        .and_then(|rows| rows.into_iter().next());
    Json(json!({ "heads": heads, "last_verification": last })).into_response()
}

async fn verify(State(state): State<AppState>, headers: HeaderMap, Path(db): Path<String>) -> Response {
    let org = match guard(&state, &headers, &db).await {
        Ok(org) => org,
        Err(response) => return response,
    };
    match aether_core::access::chain_service::verify_organization(&org).await {
        Ok(reports) => {
            let ok = reports.iter().all(|r| r.ok);
            Json(json!({ "ok": ok, "chains": reports })).into_response()
        }
        Err(error) => database_error("verify", error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_three_audit_tables_can_be_named() {
        assert_eq!(Table::parse("data_access"), Some(Table::DataAccess));
        assert_eq!(Table::parse("plugin_calls"), Some(Table::PluginCalls));
        assert_eq!(Table::parse("page_visits"), Some(Table::PageVisits));
        assert_eq!(Table::parse("users"), None);
        assert_eq!(Table::parse("data_access; REMOVE TABLE users"), None);
    }

    #[test]
    fn filters_only_use_bound_values() {
        for table in [Table::DataAccess, Table::PluginCalls, Table::PageVisits] {
            let sql = filter_sql(table);
            assert!(sql.contains("$actor") && sql.contains("$request") && sql.contains("$q") && sql.contains("$from"));
        }
        assert!(filter_sql(Table::DataAccess).contains("$operation"));
        assert!(!filter_sql(Table::DataAccess).contains("$failed"));
        assert!(filter_sql(Table::PageVisits).contains("$failed"));
    }

    #[test]
    fn times_must_be_rfc_3339() {
        assert_eq!(
            instant(&Some("2026-11-02T09:00:00+02:00".into()), "from").unwrap().as_deref(),
            Some("2026-11-02T07:00:00+00:00")
        );
        assert_eq!(instant(&Some("  ".into()), "from").unwrap(), None);
        assert!(instant(&Some("yesterday".into()), "from").is_err());
    }
}
