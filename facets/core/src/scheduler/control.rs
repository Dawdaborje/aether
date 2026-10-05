//! The control API of a standalone scheduler, and the client that talks to it.
//!
//! The HTTP server finds a standalone scheduler through the core database (`scheduler_nodes`),
//! so nothing about it needs to be written in `aether.toml`. The server pushes small messages
//! here, `wake` (a job was enqueued) and `reload` (plugins changed), so work starts at once rather
//! than at the scheduler's next look. They are hints: a scheduler that misses one finds the work
//! on its next look anyway.
//!
//! Requests carry `Authorization: Bearer <token>`. The token is a secret setting
//! (`scheduler.control_token`) created the first time it is needed, so every Aether process
//! that shares the database and the secret key knows it. The API binds to `127.0.0.1` unless
//! `[scheduler] bind` says otherwise; put it on a private network if it must be reachable
//! from other machines.

use std::{sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
};
use rand::RngExt;
use serde_json::{Value, json};
use tokio::sync::watch;

use crate::{application::settings::get_setting_plain_in, state::AppState};

use super::{
    Remote,
    nodes,
    queue::{self, NewJob},
    worker::WorkerOptions,
};

const TOKEN_SETTING: &str = "scheduler.control_token";
/// How long a hint may take to be delivered before it is dropped.
const PUSH_TIMEOUT: Duration = Duration::from_secs(2);
/// How often the HTTP server looks for a standalone scheduler.
const DISCOVER_EVERY: Duration = Duration::from_secs(10);

/// The token that authorizes control requests, created on first use.
pub async fn control_token(state: &AppState) -> Result<String, String> {
    let read = |state: &AppState| {
        let state = state.clone();
        async move {
            get_setting_plain_in(&state, TOKEN_SETTING, crate::application::settings::Scope::Global)
                .await
                .map(|value| value.and_then(|v| v.as_str().map(str::to_string)))
                .map_err(|e| e.to_string())
        }
    };
    if let Some(token) = read(state).await? {
        return Ok(token);
    }
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    let token: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    let sealed = state.secrets().map_err(|e| e.to_string())?.encrypt(&token);
    let core = state.core().await.map_err(|e| e.to_string())?;
    // Two processes may race to create it; the unique key lets one win and the other read it.
    let created = core
        .query(
            "CREATE gl_settings_items SET label = 'Scheduler control token', s_key = $key, s_value = $value, is_secret = true, \
             description = 'Authorizes requests to a standalone scheduler. Created automatically.';",
        )
        .bind(("key", TOKEN_SETTING))
        .bind(("value", sealed))
        .await
        .and_then(|response| response.check());
    match created {
        Ok(_) => Ok(token),
        Err(_) => read(state).await?.ok_or_else(|| "the scheduler control token could not be created".to_string()),
    }
}

/// Send a hint to a standalone scheduler. Failures are logged and otherwise ignored.
pub async fn push(remote: &Remote, action: &str, body: &Value) {
    let client = match reqwest::Client::builder().timeout(PUSH_TIMEOUT).no_proxy().build() {
        Ok(client) => client,
        Err(error) => {
            log::warn!("scheduler hint not sent: {error}");
            return;
        }
    };
    let result = client
        .post(format!("http://{}/control/{action}", remote.address))
        .bearer_auth(&remote.token)
        .json(body)
        .send()
        .await;
    match result {
        Ok(response) if response.status().is_success() => {}
        Ok(response) => log::warn!("scheduler at {} answered {} to `{action}`", remote.address, response.status()),
        Err(error) => log::debug!("scheduler at {} could not be reached for `{action}`: {error}", remote.address),
    }
}

/// Keep `state.scheduler`'s remote up to date: where the live standalone scheduler is, or none.
/// Run by the HTTP server for as long as it lives.
pub fn spawn_discovery(state: &AppState) -> tokio::task::JoinHandle<()> {
    let state = state.clone();
    tokio::spawn(async move {
        loop {
            if let Ok(core) = state.core().await {
                let found = match nodes::standalone_address(&core).await {
                    Ok(Some(address)) => match control_token(&state).await {
                        Ok(token) => Some(Remote { address, token }),
                        Err(error) => {
                            log::warn!("cannot reach the standalone scheduler: {error}");
                            None
                        }
                    },
                    _ => None,
                };
                if found != state.scheduler.remote() {
                    match &found {
                        Some(remote) => log::info!("Standalone scheduler found at {}", remote.address),
                        None => log::info!("No standalone scheduler is running"),
                    }
                    state.scheduler.set_remote(found);
                }
            }
            tokio::time::sleep(DISCOVER_EVERY).await;
        }
    })
}

#[derive(Clone)]
struct Control {
    state: AppState,
    token: Arc<String>,
    node_id: String,
    queues: Vec<String>,
}

/// Whether two strings are equal, without stopping at the first difference.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn authorize(State(control): State<Control>, request: Request, next: Next) -> Result<Response, StatusCode> {
    let given = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or("");
    if given.is_empty() || !same(given, &control.token) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(next.run(request).await)
}

fn router(control: Control) -> Router {
    Router::new()
        .route("/control/status", get(status))
        .route("/control/wake", post(wake))
        .route("/control/reload", post(reload))
        .route("/control/pause", post(pause))
        .route("/control/resume", post(resume))
        .route("/control/run", post(run_task))
        .layer(middleware::from_fn_with_state(control.clone(), authorize))
        .with_state(control)
}

async fn status(State(control): State<Control>) -> Result<Json<Value>, StatusCode> {
    let core = control.state.core().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut orgs = Vec::new();
    let mut response = core.query("SELECT VALUE db_name FROM organizations;").await.and_then(|r| r.check()).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let names: Vec<String> = response.take(0).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    for name in names {
        let Ok(db) = control.state.org(&name).await else { continue };
        let counts = db
            .query("SELECT state, count() AS n FROM jobs GROUP BY state;")
            .await
            .and_then(|r| r.check())
            .and_then(|mut r| r.take::<Vec<Value>>(0))
            .unwrap_or_default();
        orgs.push(json!({ "organization": name, "jobs": counts }));
    }
    Ok(Json(json!({
        "node": control.node_id,
        "queues": control.queues,
        "paused": control.state.scheduler.is_paused(),
        "nodes": nodes::alive(&core).await.unwrap_or_default(),
        "organizations": orgs,
    })))
}

async fn wake(State(control): State<Control>, Json(body): Json<Value>) -> StatusCode {
    match body.get("org").and_then(Value::as_str) {
        Some(org) => control.state.scheduler.wake_local(org),
        None => control.state.scheduler.reload_local(),
    }
    StatusCode::NO_CONTENT
}

async fn reload(State(control): State<Control>) -> StatusCode {
    control.state.scheduler.reload_local();
    StatusCode::NO_CONTENT
}

async fn pause(State(control): State<Control>) -> StatusCode {
    control.state.scheduler.set_paused(true);
    log::info!("Scheduler paused by a control request");
    StatusCode::NO_CONTENT
}

async fn resume(State(control): State<Control>) -> StatusCode {
    control.state.scheduler.set_paused(false);
    log::info!("Scheduler resumed by a control request");
    StatusCode::NO_CONTENT
}

/// Run a scheduled task now, in addition to its normal schedule.
async fn run_task(State(control): State<Control>, Json(body): Json<Value>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let field = |name: &str| body.get(name).and_then(Value::as_str).map(str::to_string);
    let (Some(org), Some(plugin), Some(task)) = (field("org"), field("plugin"), field("task")) else {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "give `org`, `plugin` and `task`" }))));
    };
    let internal = |e: &dyn std::fmt::Display| {
        log::error!("control run: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "database error" })))
    };
    let db = control.state.org(&org).await.map_err(|e| internal(&e))?;
    let mut response = db
        .query("SELECT function_name AS function, payload, queue, max_attempts FROM ONLY scheduled_tasks WHERE plugin = $plugin AND name = $name LIMIT 1;")
        .bind(("plugin", plugin.clone()))
        .bind(("name", task.clone()))
        .await
        .and_then(|r| r.check())
        .map_err(|e| internal(&e))?;
    let row: Option<Value> = response.take(0).map_err(|e| internal(&e))?;
    let Some(row) = row else {
        return Err((StatusCode::NOT_FOUND, Json(json!({ "error": "no such task" }))));
    };
    let mut job = NewJob::new("plugin", row["queue"].as_str().unwrap_or("default"));
    job.plugin = Some(plugin);
    job.function = row["function"].as_str().map(str::to_string);
    job.payload = row.get("payload").filter(|p| p.is_object()).cloned();
    job.max_attempts = row["max_attempts"].as_i64().unwrap_or(3);
    job.task = Some(task);
    job.enqueued_by = Some("system:control".into());
    let enqueued = queue::enqueue(&db, job).await.map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))))?;
    control.state.scheduler.wake_local(&org);
    Ok(Json(json!({ "job": enqueued.id })))
}

/// Serve the control API on `bind` until `shutdown` turns true.
pub async fn serve(
    state: AppState,
    bind: &str,
    options: &WorkerOptions,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), String> {
    let token = control_token(&state).await?;
    let listener = tokio::net::TcpListener::bind(bind).await.map_err(|e| format!("cannot listen on {bind}: {e}"))?;
    log::info!("Scheduler control API on http://{}", listener.local_addr().map_err(|e| e.to_string())?);
    let control = Control { state, token: Arc::new(token), node_id: options.node_id.clone(), queues: options.queues.clone() };
    axum::serve(listener, router(control))
        .with_graceful_shutdown(async move {
            let _ = shutdown.wait_for(|stopping| *stopping).await;
        })
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_compare_whole() {
        assert!(same("abc123", "abc123"));
        assert!(!same("abc123", "abc124"));
        assert!(!same("abc", "abc123"));
        assert!(!same("", "x"));
    }
}
