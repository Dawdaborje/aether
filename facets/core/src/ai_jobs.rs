//! The job runner for `ai_jobs`. The rows and their queries are in `aether_kernel::ai_jobs`.

use aether_kernel::ai_jobs::*;
pub use aether_kernel::ai_jobs::*;

use std::time::Duration;
use aether_ai::{ChatRequest, ChatResponse, Limits, gate};
use serde_json::Value;
use crate::scheduler::queue::{Job, NewJob};
use crate::databases::Db;
use crate::state::AppState;


/// Queue the plugin functions that asked to be told. One failing to queue does not stop the others.
pub async fn fire_callbacks(db: &Db, row: &Value, scheduler: &crate::scheduler::SchedulerLink, org_db: &str) {
    for callback in callbacks_of(row) {
        let mut job = NewJob::new("plugin", "default");
        job.plugin = Some(callback.plugin.clone());
        job.function = Some(callback.function.clone());
        job.payload = Some(callback_payload(row, &callback));
        job.enqueued_by = Some("system:ai".into());
        if let Err(error) = crate::scheduler::queue::enqueue(db, job).await {
            log::warn!("{org_db}: ai callback to `{}.{}` could not be queued: {error}", callback.plugin, callback.function);
        }
    }
    scheduler.wake(org_db);
}

/// What the worker needs to say about a failed attempt: the message, and whether trying again cannot help.
pub type Failure = (String, bool);

/// Run one `ai` job.
pub async fn run(state: &AppState, org_db: &str, job: &Job) -> Result<(), Failure> {
    let payload = job.payload.clone().unwrap_or(Value::Null);
    let job_id = payload["job_id"].as_str().ok_or(("the job names no result".to_string(), true))?.to_string();
    let request: ChatRequest = serde_json::from_value(payload["request"].clone()).map_err(|e| (format!("the request: {e}"), true))?;
    let who = payload["by"].as_str().map(str::to_string);
    let org = state.org(org_db).await.map_err(|e| (format!("the database: {e}"), false))?;
    let core = state.core().await.ok();
    let limits = limits_for(&org, core.as_ref()).await;
    let final_attempt = job.attempts >= job.max_attempts;
    let _ = set_status(&org, &job_id, "running", None).await;

    let outcome = call_provider(state, org_db, &org, &request, &limits, who.as_deref(), job.plugin.as_deref()).await;
    match outcome {
        Ok(response) => {
            store_done(&org, &job_id, &response).await.map_err(|e| (format!("the answer could not be kept: {e}"), false))?;
            if let Ok(Some(row)) = find(&org, &job_id).await {
                fire_callbacks(&org, &row, &state.scheduler, org_db).await;
            }
            Ok(())
        }
        Err((message, permanent)) => {
            if permanent || final_attempt {
                let _ = set_status(&org, &job_id, "failed", Some(&message)).await;
                if let Ok(Some(row)) = find(&org, &job_id).await {
                    fire_callbacks(&org, &row, &state.scheduler, org_db).await;
                }
            } else {
                let _ = set_status(&org, &job_id, "pending", Some(&message)).await;
            }
            Err((message, permanent))
        }
    }
}

async fn call_provider(
    state: &AppState,
    org_db: &str,
    org: &Db,
    request: &ChatRequest,
    limits: &Limits,
    who: Option<&str>,
    plugin: Option<&str>,
) -> Result<ChatResponse, Failure> {
    if !limits.enabled {
        return Err(("AI is turned off for this organization".into(), true));
    }
    request.validate(limits).map_err(|e| (e, true))?;
    let provider = limits.chosen_provider().map_err(|e| (e, true))?;
    check_budget(org, limits, who).await.map_err(|e| (e, true))?;
    // A job may wait for a free place much longer than a plugin call can.
    let permit = gate()
        .acquire(limits.max_concurrent, Duration::from_secs(60))
        .await
        .ok_or(("the AI provider is busy".to_string(), false))?;
    let started = std::time::Instant::now();
    let answer =
        crate::bridges::call_internal(state, org_db, provider.key, "chat", &chat_params(request, limits, limits.job_timeout_secs)).await;
    drop(permit);
    let elapsed = started.elapsed().as_millis() as u64;
    let parsed = answer.map_err(|e| (e.message, e.permanent)).and_then(|v| parse_response(v).map_err(|e| (e, true)));
    let (input, output) = parsed.as_ref().map(|r| (r.usage.input_tokens, r.usage.output_tokens)).unwrap_or((0, 0));
    record_usage(
        org,
        Usage {
            who,
            plugin,
            kind: "complete",
            provider: provider.key,
            model: parsed.as_ref().ok().map(|r| r.model.as_str()).or(request.model.as_deref()),
            input_tokens: input,
            output_tokens: output,
            duration_ms: elapsed,
            ok: parsed.is_ok(),
        },
    )
    .await;
    parsed
}

