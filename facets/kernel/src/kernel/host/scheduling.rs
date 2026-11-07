//! `scheduler::enqueue`, `scheduler::job`, `scheduler::cancel_job`, `scheduler::register` and
//! `scheduler::cancel`: background work for a plugin.
//!
//! * **Jobs** run one of the plugin's *own* functions later, in the background, with retries.
//!   They run as the kernel (`system:scheduler`) under the plugin's own capabilities and model
//!   grants, never as the person who enqueued them. A plugin holding `scheduler::run_as` may name an
//!   account in `run_as`: that job runs as the person, under their roles and record rules. To involve another plugin, the function
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
    /// Run as this account (`users:…`) under their roles and record rules, not as the kernel.
    #[serde(default)]
    run_as: Option<String>,
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
    serde_json::from_value(payload.clone())
        .map_err(|error| HostError::InvalidPayload(error.to_string()))
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

/// The job a request describes, checked as `scheduler::enqueue` checks it.
fn new_job(
    ctx: &PluginHostContext,
    request: EnqueueRequest,
    defaults: &super::context::JobDefaults,
) -> Result<NewJob, HostError> {
    if request.function.is_empty() || request.function.len() > 128 {
        return Err(HostError::InvalidPayload(
            "`function` names one of this plugin's functions".into(),
        ));
    }
    let delay = request.delay_secs.unwrap_or(0);
    if !(0..=MAX_DELAY_SECS).contains(&delay) {
        return Err(HostError::InvalidPayload(
            "`delay_secs` is 0 to a year".into(),
        ));
    }
    let mut job = NewJob::new("plugin", request.queue.unwrap_or_else(|| "default".into()));
    job.plugin = Some(ctx.plugin_name.clone());
    job.function = Some(request.function);
    job.payload = request.payload;
    job.run_at = Utc::now() + Duration::seconds(delay);
    job.max_attempts = request.max_attempts.unwrap_or(defaults.max_attempts);
    job.backoff_secs = defaults.backoff_secs;
    job.unique_key = request.unique_key;
    if let Some(user) = request.run_as.filter(|user| !user.is_empty()) {
        // Acting as a person is the sensitive part, so it has a capability of its own.
        ctx.require_cap("scheduler::run_as")?;
        if !user.starts_with("users:") {
            return Err(HostError::InvalidPayload(
                "`run_as` is an account id such as `users:…`".into(),
            ));
        }
        job.run_as = Some(user);
    }
    job.enqueued_by = ctx
        .audit
        .actor
        .id()
        .map(str::to_string)
        .or_else(|| Some("anonymous".into()));
    job.request_id = Some(ctx.audit.request_id.clone());
    Ok(job)
}

/// Queue a call to one of the plugin's own functions.
pub async fn enqueue(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("scheduler::enqueue")?;
    let request: EnqueueRequest = parse(payload)?;
    let scheduler = ctx
        .services()?
        .scheduler
        .clone()
        .ok_or_else(|| HostError::Message("background jobs are not available here".into()))?;
    let defaults = scheduler.defaults(&ctx.database).await;
    let job = new_job(ctx, request, &defaults)?;
    let enqueued = queue::enqueue(&ctx.db, job).await.map_err(queue_error)?;
    if enqueued.created {
        scheduler.wake(&ctx.database);
    }
    Ok(
        serde_json::json!({ "ok": true, "data": { "id": enqueued.id, "created": enqueued.created } }),
    )
}

#[derive(Deserialize)]
struct EnqueueManyRequest {
    jobs: Vec<EnqueueRequest>,
}

/// Queue many calls at once: `{ "jobs": [ <what scheduler::enqueue takes>, … ] }`, up to 10 000. One round trip
/// from the plugin and a few to the database, instead of two per job; what a fan-out uses. Answers
/// `{ "jobs": [ { "id", "created" }, … ] }` in the same order. Same capability as `scheduler::enqueue`.
pub async fn enqueue_many(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("scheduler::enqueue")?;
    let request: EnqueueManyRequest = parse(payload)?;
    if request.jobs.len() > queue::MAX_JOBS_PER_ENQUEUE {
        return Err(HostError::InvalidPayload(format!(
            "`jobs` holds at most {} jobs",
            queue::MAX_JOBS_PER_ENQUEUE
        )));
    }
    let scheduler = ctx
        .services()?
        .scheduler
        .clone()
        .ok_or_else(|| HostError::Message("background jobs are not available here".into()))?;
    let defaults = scheduler.defaults(&ctx.database).await;
    let mut jobs = Vec::with_capacity(request.jobs.len());
    for (index, job) in request.jobs.into_iter().enumerate() {
        jobs.push(new_job(ctx, job, &defaults).map_err(|error| match error {
            HostError::InvalidPayload(message) => HostError::InvalidPayload(format!("job {index}: {message}")),
            other => other,
        })?);
    }
    let enqueued = queue::enqueue_many(&ctx.db, jobs).await.map_err(queue_error)?;
    if enqueued.iter().any(|job| job.created) {
        scheduler.wake(&ctx.database);
    }
    let jobs: Vec<JsonValue> = enqueued
        .into_iter()
        .map(|job| serde_json::json!({ "id": job.id, "created": job.created }))
        .collect();
    Ok(serde_json::json!({ "ok": true, "data": { "jobs": jobs } }))
}

/// How one of the plugin's jobs is doing.
pub async fn job(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("scheduler::enqueue")?;
    let request: IdRequest = parse(payload)?;
    let status = queue::status(&ctx.db, &ctx.plugin_name, &request.id)
        .await
        .map_err(queue_error)?;
    Ok(serde_json::json!({ "ok": true, "data": status }))
}

/// Cancel one of the plugin's jobs that has not started.
pub async fn cancel_job(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("scheduler::enqueue")?;
    let request: IdRequest = parse(payload)?;
    let cancelled = queue::cancel(&ctx.db, &ctx.plugin_name, &request.id)
        .await
        .map_err(queue_error)?;
    Ok(serde_json::json!({ "ok": true, "data": { "cancelled": cancelled } }))
}

/// Add or update a recurring task.
pub async fn register(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("scheduler::register")?;
    let def: TaskDef = parse(payload)?;
    tasks::register_runtime(&ctx.db, &ctx.plugin_name, &def)
        .await
        .map_err(task_error)?;
    if let Some(scheduler) = ctx.services()?.scheduler.clone() {
        scheduler.reload();
    }
    Ok(serde_json::json!({ "ok": true, "data": null }))
}

/// Remove a task that was registered while running.
pub async fn cancel(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("scheduler::cancel")?;
    let request: NameRequest = parse(payload)?;
    let removed = tasks::cancel_runtime(&ctx.db, &ctx.plugin_name, &request.name)
        .await
        .map_err(task_error)?;
    Ok(serde_json::json!({ "ok": true, "data": { "removed": removed } }))
}
