//! Desk HTTP API for an organization's scheduled tasks and jobs.
//!
//! Developers only: unrestricted ones (`users.is_super_user`) in every organization, org-scoped
//! ones (`organization_users.is_developer`) in theirs. Org is always an explicit `{db}` path
//! segment so lists never mix organizations, and it is what access is checked against. Tasks are filtered to plugins installed in that org.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use surrealdb::types::SurrealValue;

use crate::application::settings_api::require_org_developer;
use crate::scheduler::tasks::{self, TaskError};
use crate::scheduler::worker;
use crate::state::AppState;
use crate::tenancy::OrgRef;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/ui/organizations/{db}/schedules/tasks",
            get(list_tasks),
        )
        .route(
            "/api/ui/organizations/{db}/schedules/tasks/{plugin}/{name}",
            get(get_task),
        )
        .route(
            "/api/ui/organizations/{db}/schedules/tasks/{plugin}/{name}/runs",
            get(list_runs),
        )
        .route(
            "/api/ui/organizations/{db}/schedules/tasks/{plugin}/{name}/stop",
            post(stop_task),
        )
        .route(
            "/api/ui/organizations/{db}/schedules/tasks/{plugin}/{name}/restart",
            post(restart_task),
        )
        .route(
            "/api/ui/organizations/{db}/schedules/tasks/{plugin}/{name}/run",
            post(run_task),
        )
        .route("/api/ui/organizations/{db}/schedules/jobs", get(list_jobs))
        .route(
            "/api/ui/organizations/{db}/schedules/jobs/{id}",
            get(get_job),
        )
}

fn failure(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn database_error(context: &str, error: impl std::fmt::Display) -> Response {
    log::error!("schedules {context}: {error}");
    failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
}

/// An unrestricted developer, or a developer of this organization only.
async fn require_developer(
    state: &AppState,
    headers: &HeaderMap,
    db: &str,
) -> Result<(), Response> {
    require_org_developer(state, headers, db)
        .await
        .map_err(|(status, body)| (status, body).into_response())
}

async fn open_org(state: &AppState, db: &str) -> Result<crate::state::Db, Response> {
    let core = state.core().await.map_err(|e| database_error("core", e))?;
    let mut response = core
        .query("SELECT db_name FROM org_databases WHERE db_name = $org LIMIT 1;")
        .bind(("org", db.to_string()))
        .await
        .and_then(|r| r.check())
        .map_err(|e| database_error("org lookup", e))?;
    let found: Option<Value> = response
        .take(0)
        .map_err(|e| database_error("org lookup", e))?;
    if found.is_none() {
        return Err(failure(StatusCode::NOT_FOUND, "organization not found"));
    }
    state
        .org(db)
        .await
        .map_err(|e| database_error("open org", e))
}

async fn installed_plugins(
    state: &AppState,
    org_name: &str,
    org: &crate::state::Db,
) -> Result<Vec<String>, Response> {
    let rows = crate::plugin_manager::installed::list_cached(&state.cache, org_name, org)
        .await
        .map_err(|e| database_error("installed plugins", e))?;
    Ok(rows.into_iter().map(|row| row.plugin_name).collect())
}

#[derive(Debug, Deserialize, SurrealValue)]
struct TaskDbRow {
    plugin: String,
    name: String,
    function_name: String,
    title: Option<String>,
    description: Option<String>,
    cron: Option<String>,
    every_secs: Option<i64>,
    timezone: Option<String>,
    queue: String,
    max_attempts: i64,
    source: String,
    enabled: bool,
    catch_up: String,
    next_run: Option<String>,
    last_run: Option<String>,
}

fn task_json(row: TaskDbRow) -> Value {
    let kind = if row.cron.is_some() {
        "cron"
    } else {
        "interval"
    };
    json!({
        "plugin": row.plugin,
        "name": row.name,
        "function": row.function_name,
        "title": row.title,
        "description": row.description,
        "kind": kind,
        "cron": row.cron,
        "every_secs": row.every_secs,
        "timezone": row.timezone,
        "queue": row.queue,
        "max_attempts": row.max_attempts,
        "source": row.source,
        "enabled": row.enabled,
        "catch_up": row.catch_up,
        "next_run": row.next_run,
        "last_run": row.last_run,
    })
}

async fn list_tasks(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(db): Path<String>,
) -> Response {
    if let Err(response) = require_developer(&state, &headers, &db).await {
        return response;
    }
    let org = match open_org(&state, &db).await {
        Ok(org) => org,
        Err(response) => return response,
    };
    let plugins = match installed_plugins(&state, &db, &org).await {
        Ok(plugins) => plugins,
        Err(response) => return response,
    };
    let mut response = match org
        .query(
            "SELECT plugin, name, function_name, title, description, cron, every_secs, timezone, queue, max_attempts, source, enabled, catch_up, \
             <string> next_run AS next_run, <string> last_run AS last_run \
             FROM scheduled_tasks WHERE plugin IN $plugins ORDER BY plugin, name;",
        )
        .bind(("plugins", plugins))
        .await
        .and_then(|r| r.check())
    {
        Ok(response) => response,
        Err(error) => return database_error("list tasks", error),
    };
    let rows: Vec<TaskDbRow> = match response.take(0) {
        Ok(rows) => rows,
        Err(error) => return database_error("list tasks", error),
    };
    Json(json!({ "tasks": rows.into_iter().map(task_json).collect::<Vec<_>>() })).into_response()
}

async fn get_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((db, plugin, name)): Path<(String, String, String)>,
) -> Response {
    if let Err(response) = require_developer(&state, &headers, &db).await {
        return response;
    }
    let org = match open_org(&state, &db).await {
        Ok(org) => org,
        Err(response) => return response,
    };
    let plugins = match installed_plugins(&state, &db, &org).await {
        Ok(plugins) => plugins,
        Err(response) => return response,
    };
    if !plugins.iter().any(|p| p == &plugin) {
        return failure(StatusCode::NOT_FOUND, "no such task");
    }
    let mut response = match org
        .query(
            "SELECT plugin, name, function_name, title, description, cron, every_secs, timezone, queue, max_attempts, source, enabled, catch_up, \
             <string> next_run AS next_run, <string> last_run AS last_run \
             FROM ONLY scheduled_tasks WHERE plugin = $plugin AND name = $name LIMIT 1;",
        )
        .bind(("plugin", plugin.clone()))
        .bind(("name", name.clone()))
        .await
        .and_then(|r| r.check())
    {
        Ok(response) => response,
        Err(error) => return database_error("get task", error),
    };
    let row: Option<TaskDbRow> = match response.take(0) {
        Ok(row) => row,
        Err(error) => return database_error("get task", error),
    };
    let Some(row) = row else {
        return failure(StatusCode::NOT_FOUND, "no such task");
    };
    let mut counts_response = match org
        .query("SELECT state, count() AS n FROM jobs WHERE plugin = $plugin AND task = $name GROUP BY state;")
        .bind(("plugin", plugin))
        .bind(("name", name))
        .await
        .and_then(|r| r.check())
    {
        Ok(response) => response,
        Err(error) => return database_error("run counts", error),
    };
    let counts: Vec<Value> = counts_response.take(0).unwrap_or_default();
    Json(json!({ "task": task_json(row), "run_counts": counts })).into_response()
}

#[derive(Debug, Deserialize)]
struct RunsQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct RunRow {
    id: String,
    state: String,
    attempts: i64,
    max_attempts: i64,
    last_error: Option<String>,
    date_started: Option<String>,
    date_finished: Option<String>,
    run_at: Option<String>,
    date_created: Option<String>,
}

fn duration_ms(started: Option<&str>, finished: Option<&str>) -> Option<i64> {
    let (Some(started), Some(finished)) = (started, finished) else {
        return None;
    };
    let started = DateTime::parse_from_rfc3339(started)
        .ok()?
        .with_timezone(&Utc);
    let finished = DateTime::parse_from_rfc3339(finished)
        .ok()?
        .with_timezone(&Utc);
    Some((finished - started).num_milliseconds().max(0))
}

fn run_json(row: RunRow) -> Value {
    let duration = duration_ms(row.date_started.as_deref(), row.date_finished.as_deref());
    json!({
        "id": row.id,
        "state": row.state,
        "attempts": row.attempts,
        "max_attempts": row.max_attempts,
        "last_error": row.last_error,
        "date_started": row.date_started,
        "date_finished": row.date_finished,
        "run_at": row.run_at,
        "date_created": row.date_created,
        "duration_ms": duration,
    })
}

async fn list_runs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((db, plugin, name)): Path<(String, String, String)>,
    Query(query): Query<RunsQuery>,
) -> Response {
    if let Err(response) = require_developer(&state, &headers, &db).await {
        return response;
    }
    let org = match open_org(&state, &db).await {
        Ok(org) => org,
        Err(response) => return response,
    };
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let offset = query.offset.unwrap_or(0).max(0);
    let mut response = match org
        .query(
            "SELECT <string> record::id(id) AS id, state, attempts, max_attempts, last_error, \
             <string> date_started AS date_started, <string> date_finished AS date_finished, \
             <string> run_at AS run_at, <string> date_created AS date_created \
             FROM jobs WHERE plugin = $plugin AND task = $name \
             ORDER BY date_created DESC LIMIT $limit START $offset;",
        )
        .bind(("plugin", plugin))
        .bind(("name", name))
        .bind(("limit", limit))
        .bind(("offset", offset))
        .await
        .and_then(|r| r.check())
    {
        Ok(response) => response,
        Err(error) => return database_error("list runs", error),
    };
    let rows: Vec<RunRow> = match response.take(0) {
        Ok(rows) => rows,
        Err(error) => return database_error("list runs", error),
    };
    Json(json!({ "runs": rows.into_iter().map(run_json).collect::<Vec<_>>() })).into_response()
}

#[derive(Debug, Deserialize)]
struct JobsQuery {
    limit: Option<i64>,
    state: Option<String>,
    plugin: Option<String>,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct JobListRow {
    id: String,
    kind: String,
    plugin: Option<String>,
    function_name: Option<String>,
    task: Option<String>,
    queue: String,
    state: String,
    attempts: i64,
    max_attempts: i64,
    last_error: Option<String>,
    date_started: Option<String>,
    date_finished: Option<String>,
    run_at: Option<String>,
    date_created: Option<String>,
}

async fn list_jobs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(db): Path<String>,
    Query(query): Query<JobsQuery>,
) -> Response {
    if let Err(response) = require_developer(&state, &headers, &db).await {
        return response;
    }
    let org = match open_org(&state, &db).await {
        Ok(org) => org,
        Err(response) => return response,
    };
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let mut response = match org
        .query(
            "SELECT <string> record::id(id) AS id, kind, plugin, function_name, task, queue, state, attempts, max_attempts, last_error, \
             <string> date_started AS date_started, <string> date_finished AS date_finished, \
             <string> run_at AS run_at, <string> date_created AS date_created \
             FROM jobs \
             WHERE ($state = NONE OR state = $state) AND ($plugin = NONE OR plugin = $plugin) \
             ORDER BY date_created DESC LIMIT $limit;",
        )
        .bind(("limit", limit))
        .bind(("state", query.state.clone()))
        .bind(("plugin", query.plugin.clone()))
        .await
        .and_then(|r| r.check())
    {
        Ok(response) => response,
        Err(error) => return database_error("list jobs", error),
    };
    let rows: Vec<JobListRow> = match response.take(0) {
        Ok(rows) => rows,
        Err(error) => return database_error("list jobs", error),
    };
    let mut counts_response = match org
        .query("SELECT state, count() AS n FROM jobs GROUP BY state;")
        .await
        .and_then(|r| r.check())
    {
        Ok(response) => response,
        Err(error) => return database_error("job counts", error),
    };
    let counts: Vec<Value> = counts_response.take(0).unwrap_or_default();
    let jobs: Vec<Value> = rows
        .into_iter()
        .map(|row| {
            let duration = duration_ms(row.date_started.as_deref(), row.date_finished.as_deref());
            json!({
                "id": row.id,
                "kind": row.kind,
                "plugin": row.plugin,
                "function": row.function_name,
                "task": row.task,
                "queue": row.queue,
                "state": row.state,
                "attempts": row.attempts,
                "max_attempts": row.max_attempts,
                "last_error": row.last_error,
                "date_started": row.date_started,
                "date_finished": row.date_finished,
                "run_at": row.run_at,
                "date_created": row.date_created,
                "duration_ms": duration,
            })
        })
        .collect();
    Json(json!({ "jobs": jobs, "counts": counts })).into_response()
}

#[derive(Debug, Deserialize, SurrealValue)]
struct PluginCallRow {
    plugin: String,
    function_name: String,
    status: i64,
    date_created: Option<String>,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct DataAccessRow {
    plugin: String,
    function_name: String,
    model: Option<String>,
    table_name: Option<String>,
    operation: String,
    record_count: i64,
    statement: Option<String>,
    date_created: Option<String>,
}

async fn get_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((db, id)): Path<(String, String)>,
) -> Response {
    if let Err(response) = require_developer(&state, &headers, &db).await {
        return response;
    }
    if id.trim().is_empty() || id.contains(':') {
        return failure(StatusCode::BAD_REQUEST, "invalid job id");
    }
    let org = match open_org(&state, &db).await {
        Ok(org) => org,
        Err(response) => return response,
    };
    let mut response = match org
        .query(
            "SELECT <string> record::id(id) AS id, kind, plugin, function_name, task, queue, state, attempts, max_attempts, last_error, \
             <string> date_started AS date_started, <string> date_finished AS date_finished, \
             <string> run_at AS run_at, <string> date_created AS date_created \
             FROM ONLY type::record('jobs', $id);",
        )
        .bind(("id", id.clone()))
        .await
        .and_then(|r| r.check())
    {
        Ok(response) => response,
        Err(error) => return database_error("get job", error),
    };
    let row: Option<JobListRow> = match response.take(0) {
        Ok(row) => row,
        Err(error) => return database_error("get job", error),
    };
    let Some(row) = row else {
        return failure(StatusCode::NOT_FOUND, "no such job");
    };
    let request_id = format!("job-{id}");
    let mut calls_response = match org
        .query(
            "SELECT plugin, function_name, status, <string> date_created AS date_created \
             FROM plugin_calls WHERE request_id = $request_id ORDER BY date_created;",
        )
        .bind(("request_id", request_id.clone()))
        .await
        .and_then(|r| r.check())
    {
        Ok(response) => response,
        Err(error) => return database_error("plugin_calls", error),
    };
    let plugin_calls: Vec<PluginCallRow> = calls_response.take(0).unwrap_or_default();
    let mut access_response = match org
        .query(
            "SELECT plugin, function_name, model, table_name, operation, record_count, statement, \
             <string> date_created AS date_created \
             FROM data_access WHERE request_id = $request_id ORDER BY date_created;",
        )
        .bind(("request_id", request_id))
        .await
        .and_then(|r| r.check())
    {
        Ok(response) => response,
        Err(error) => return database_error("data_access", error),
    };
    let data_access: Vec<DataAccessRow> = access_response.take(0).unwrap_or_default();
    let duration = duration_ms(row.date_started.as_deref(), row.date_finished.as_deref());
    Json(json!({
        "job": {
            "id": row.id,
            "kind": row.kind,
            "plugin": row.plugin,
            "function": row.function_name,
            "task": row.task,
            "queue": row.queue,
            "state": row.state,
            "attempts": row.attempts,
            "max_attempts": row.max_attempts,
            "last_error": row.last_error,
            "date_started": row.date_started,
            "date_finished": row.date_finished,
            "run_at": row.run_at,
            "date_created": row.date_created,
            "duration_ms": duration,
        },
        "logs": {
            "plugin_calls": plugin_calls.into_iter().map(|c| json!({
                "plugin": c.plugin,
                "function": c.function_name,
                "status": c.status,
                "date_created": c.date_created,
            })).collect::<Vec<_>>(),
            "data_access": data_access.into_iter().map(|a| json!({
                "plugin": a.plugin,
                "function": a.function_name,
                "model": a.model,
                "table": a.table_name,
                "operation": a.operation,
                "record_count": a.record_count,
                "statement": a.statement,
                "date_created": a.date_created,
            })).collect::<Vec<_>>(),
        }
    }))
    .into_response()
}

async fn stop_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((db, plugin, name)): Path<(String, String, String)>,
) -> Response {
    mutate_enabled(&state, &headers, &db, &plugin, &name, false).await
}

async fn restart_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((db, plugin, name)): Path<(String, String, String)>,
) -> Response {
    mutate_enabled(&state, &headers, &db, &plugin, &name, true).await
}

async fn mutate_enabled(
    state: &AppState,
    headers: &HeaderMap,
    db: &str,
    plugin: &str,
    name: &str,
    enabled: bool,
) -> Response {
    if let Err(response) = require_developer(state, headers, db).await {
        return response;
    }
    let org = match open_org(state, db).await {
        Ok(org) => org,
        Err(response) => return response,
    };
    match tasks::set_enabled(&org, plugin, name, enabled).await {
        Ok(true) => {
            if enabled {
                state.scheduler.wake(db);
            }
            Json(json!({ "ok": true, "enabled": enabled })).into_response()
        }
        Ok(false) => failure(StatusCode::NOT_FOUND, "no such task"),
        Err(TaskError::Refused(message)) => failure(StatusCode::BAD_REQUEST, &message),
        Err(error) => database_error("set enabled", error),
    }
}

async fn run_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((db, plugin, name)): Path<(String, String, String)>,
) -> Response {
    if let Err(response) = require_developer(&state, &headers, &db).await {
        return response;
    }
    let org = match open_org(&state, &db).await {
        Ok(org) => org,
        Err(response) => return response,
    };
    let org_ref = OrgRef {
        slug: db.clone(),
        db_name: db.clone(),
    };
    let backoff = worker::number_setting(&state, &org_ref, "scheduler.retry_backoff_secs", 30)
        .await
        .clamp(1, 3600);
    match tasks::enqueue_now(&org, &plugin, &name, "system:desk", backoff).await {
        Ok(enqueued) => {
            state.scheduler.wake(&db);
            Json(json!({ "job": enqueued.id, "created": enqueued.created })).into_response()
        }
        Err(TaskError::Refused(message)) if message == "no such task" => {
            failure(StatusCode::NOT_FOUND, "no such task")
        }
        Err(TaskError::Refused(message)) => failure(StatusCode::BAD_REQUEST, &message),
        Err(TaskError::Queue(crate::scheduler::QueueError::Refused(message))) => {
            failure(StatusCode::BAD_REQUEST, &message)
        }
        Err(error) => database_error("run now", error),
    }
}
