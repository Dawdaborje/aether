//! `scheduler::enqueue`, `scheduler::job`, `scheduler::cancel_job`, `scheduler::register` and
//! `scheduler::cancel`: background work for a plugin.
//!
//! * **Jobs** run one of the plugin's *own* functions later, in the background, with retries.
//!   They run as the kernel (`system:scheduler`) under the plugin's own capabilities and model
//!   grants, never as the person who enqueued them. To involve another plugin, the function
//!   calls it with `plugins::call`.
//! * **Tasks** are recurring: a cron expression or an interval. Tasks declared in `plugin.toml`
//!   (`[[schedule]]`) belong to the manifest; `scheduler::register` adds more while running.
//!
//! Delivery is at-least-once: a job may run again if its worker died partway, so a function should
//! be safe to repeat. The job's id is in `context::get` (`request_id` is `job-<id>`), and
//! `unique_key` keeps the same logical job from being queued twice.

use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::Value as JsonValue;

use crate::scheduler::{
    queue::{self, NewJob, QueueError},
    tasks::{self, TaskDef, TaskError},
};

use super::context::PluginHostContext;
use super::error::HostError;

/// Longest a job may be delayed.
const MAX_DELAY_SECS: i64 = 365 * 86_400;

#[derive(Deserialize)]
struct EnqueueRequest {
    function: String,
    #[serde(default)]
    payload: Option<JsonValue>,
    #[serde(default)]
    delay_secs: Option<i64>,
    #[serde(default)]
    queue: Option<String>,
    #[serde(default)]
    max_attempts: Option<i64>,
    #[serde(default)]
    unique_key: Option<String>,
}

#[derive(Deserialize)]
struct IdRequest {
    id: String,
}

#[derive(Deserialize)]
struct NameRequest {
    name: String,
}

fn parse<T: serde::de::DeserializeOwned>(payload: &JsonValue) -> Result<T, HostError> {
    serde_json::from_value(payload.clone()).map_err(|error| HostError::InvalidPayload(error.to_string()))
}

fn queue_error(error: QueueError) -> HostError {
    match error {
        QueueError::Refused(message) => HostError::InvalidPayload(message),
        QueueError::Db(error) => HostError::Db(error),
    }
}

fn task_error(error: TaskError) -> HostError {
    match error {
        TaskError::Db(error) => HostError::Db(error),
        TaskError::Queue(error) => queue_error(error),
        other => HostError::InvalidPayload(other.to_string()),
    }
}

/// Queue a call to one of the plugin's own functions.
pub async fn enqueue(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("scheduler::enqueue")?;
    let request: EnqueueRequest = parse(payload)?;
    if request.function.is_empty() || request.function.len() > 128 {
        return Err(HostError::InvalidPayload("`function` names one of this plugin's functions".into()));
    }
    let delay = request.delay_secs.unwrap_or(0);
    if !(0..=MAX_DELAY_SECS).contains(&delay) {
        return Err(HostError::InvalidPayload("`delay_secs` is 0 to a year".into()));
    }
    let scheduler = ctx.services()?.scheduler.clone().ok_or_else(|| HostError::Message("background jobs are not available here".into()))?;
    let defaults = scheduler.defaults(&ctx.database).await;

    let mut job = NewJob::new("plugin", request.queue.unwrap_or_else(|| "default".into()));
    job.plugin = Some(ctx.plugin_name.clone());
    job.function = Some(request.function);
    job.payload = request.payload;
    job.run_at = Utc::now() + Duration::seconds(delay);
    job.max_attempts = request.max_attempts.unwrap_or(defaults.max_attempts);
    job.backoff_secs = defaults.backoff_secs;
    job.unique_key = request.unique_key;
    job.enqueued_by = ctx.audit.actor.id().map(str::to_string).or_else(|| Some("anonymous".into()));
    job.request_id = Some(ctx.audit.request_id.clone());
    let enqueued = queue::enqueue(&ctx.db, job).await.map_err(queue_error)?;
    if enqueued.created {
        scheduler.wake(&ctx.database);
    }
    Ok(serde_json::json!({ "ok": true, "data": { "id": enqueued.id, "created": enqueued.created } }))
}

/// How one of the plugin's jobs is doing.
pub async fn job(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("scheduler::enqueue")?;
    let request: IdRequest = parse(payload)?;
    let status = queue::status(&ctx.db, &ctx.plugin_name, &request.id).await.map_err(queue_error)?;
    Ok(serde_json::json!({ "ok": true, "data": status }))
}

/// Cancel one of the plugin's jobs that has not started.
pub async fn cancel_job(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("scheduler::enqueue")?;
    let request: IdRequest = parse(payload)?;
    let cancelled = queue::cancel(&ctx.db, &ctx.plugin_name, &request.id).await.map_err(queue_error)?;
    Ok(serde_json::json!({ "ok": true, "data": { "cancelled": cancelled } }))
}

/// Add or update a recurring task.
pub async fn register(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("scheduler::register")?;
    let def: TaskDef = parse(payload)?;
    tasks::register_runtime(&ctx.db, &ctx.plugin_name, &def).await.map_err(task_error)?;
    if let Some(scheduler) = ctx.services()?.scheduler.clone() {
        scheduler.reload();
    }
    Ok(serde_json::json!({ "ok": true, "data": null }))
}

/// Remove a task that was registered while running.
pub async fn cancel(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("scheduler::cancel")?;
    let request: NameRequest = parse(payload)?;
    let removed = tasks::cancel_runtime(&ctx.db, &ctx.plugin_name, &request.name).await.map_err(task_error)?;
    Ok(serde_json::json!({ "ok": true, "data": { "removed": removed } }))
}
