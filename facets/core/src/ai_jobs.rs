//! Model calls that run as a job, the usage log, and the token budgets.
//!
//! `ai::submit` writes a row to `ai_result` and queues a job of kind `ai`; a worker calls the provider,
//! stores the answer, and calls back the plugin functions that asked (`on_done`). Every call, direct
//! or queued, adds a row to `ai_usage` (who, plugin, model, token counts, time; never the prompt or the
//! answer), which is also what the daily budgets add up. See `docs/architecture/ai.md`.

use std::time::Duration;

use aether_ai::{ChatRequest, ChatResponse, Limits, gate, keys};
use rand::RngExt;
use serde_json::{Value, json};

use crate::scheduler::queue::{Job, NewJob};
use crate::state::{AppState, Db};

/// The job kind and queue model calls use.
pub const KIND: &str = "ai";
pub const QUEUE: &str = "ai";

/// A plugin function to call when a job ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Callback {
    pub plugin: String,
    pub function: String,
    pub reference: Option<String>,
}

impl Callback {
    fn to_json(&self) -> Value {
        json!({ "plugin": self.plugin, "function": self.function, "ref": self.reference })
    }
}

/// The limits for an organization: the organization's own settings over the global ones.
pub async fn limits_for(org: &Db, core: Option<&Db>) -> Limits {
    let values = crate::kernel::host::settings_read::effective_in(org, core, keys::ALL).await;
    Limits::from_settings(|key| values.get(key).cloned())
}

/// The parameters a provider bridge is called with for a conversation.
pub fn chat_params(request: &ChatRequest, limits: &Limits, timeout_secs: u64) -> Value {
    json!({
        "messages": request.messages,
        "model": request.model.clone().or_else(|| limits.model.clone()),
        "max_tokens": request.token_cap(limits),
        "temperature": request.temperature,
        "json": request.json,
        "tools": request.tools,
        "timeout_secs": timeout_secs,
    })
}

/// A provider's answer, checked to be the common shape.
pub fn parse_response(value: Value) -> Result<ChatResponse, String> {
    serde_json::from_value(value).map_err(|error| format!("the provider's answer: {error}"))
}

/// One line of the usage log.
pub struct Usage<'a> {
    pub who: Option<&'a str>,
    pub plugin: Option<&'a str>,
    pub kind: &'a str,
    pub provider: &'a str,
    pub model: Option<&'a str>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub duration_ms: u64,
    pub ok: bool,
}

pub async fn record_usage(db: &Db, usage: Usage<'_>) {
    let tokens = usage.input_tokens.saturating_add(usage.output_tokens);
    let result = db
        .query(
            "CREATE ai_usage SET day = time::format(time::now(), '%Y-%m-%d'), who = $who, plugin = $plugin, kind = $kind, \
             provider = $provider, model = $model, input_tokens = $input, output_tokens = $output, tokens = $tokens, \
             duration_ms = $duration, ok = $ok;",
        )
        .bind(("who", usage.who.map(str::to_string)))
        .bind(("plugin", usage.plugin.map(str::to_string)))
        .bind(("kind", usage.kind.to_string()))
        .bind(("provider", usage.provider.to_string()))
        .bind(("model", usage.model.map(str::to_string)))
        .bind(("input", usage.input_tokens as i64))
        .bind(("output", usage.output_tokens as i64))
        .bind(("tokens", tokens as i64))
        .bind(("duration", usage.duration_ms as i64))
        .bind(("ok", usage.ok))
        .await
        .and_then(|r| r.check());
    if let Err(error) = result {
        log::warn!("ai: usage could not be logged: {error}");
    }
}

async fn tokens_today(db: &Db, who: Option<&str>) -> Result<u64, surrealdb::Error> {
    let query = if who.is_some() {
        "SELECT math::sum(tokens) AS n FROM ai_usage WHERE day = time::format(time::now(), '%Y-%m-%d') AND who = $who GROUP ALL;"
    } else {
        "SELECT math::sum(tokens) AS n FROM ai_usage WHERE day = time::format(time::now(), '%Y-%m-%d') GROUP ALL;"
    };
    let mut response = db.query(query).bind(("who", who.map(str::to_string))).await?.check()?;
    let rows: Vec<Value> = response.take(0)?;
    Ok(rows.first().and_then(|r| r["n"].as_u64()).unwrap_or(0))
}

/// Refuse a call when today's tokens already reach the organization's or the person's budget. A budget
/// of 0 is no budget. The call that crosses the line is allowed: tokens are known only afterwards.
pub async fn check_budget(db: &Db, limits: &Limits, who: Option<&str>) -> Result<(), String> {
    if limits.daily_tokens_org > 0 {
        let used = tokens_today(db, None).await.map_err(|e| format!("the usage could not be read ({e})"))?;
        if used >= limits.daily_tokens_org {
            return Err("this organization's AI budget for today is used up".into());
        }
    }
    if limits.daily_tokens_user > 0 {
        if let Some(who) = who {
            let used = tokens_today(db, Some(who)).await.map_err(|e| format!("the usage could not be read ({e})"))?;
            if used >= limits.daily_tokens_user {
                return Err("your AI budget for today is used up".into());
            }
        }
    }
    Ok(())
}

/// The stored row, as JSON, or `None`.
pub async fn find(db: &Db, job_id: &str) -> Result<Option<Value>, surrealdb::Error> {
    let mut response = db
        .query("SELECT job_id, status, result, error, plugin, callbacks, requested_by, date_updated FROM ai_result WHERE job_id = $id LIMIT 1;")
        .bind(("id", job_id.to_string()))
        .await?
        .check()?;
    let rows: Vec<Value> = response.take(0)?;
    Ok(rows.into_iter().next())
}

/// How many jobs are waiting or running.
pub async fn in_flight(db: &Db) -> Result<u64, surrealdb::Error> {
    let mut response = db
        .query("SELECT count() AS n FROM ai_result WHERE status IN ['pending', 'running'] GROUP ALL;")
        .await?
        .check()?;
    let rows: Vec<Value> = response.take(0)?;
    Ok(rows.first().and_then(|r| r["n"].as_u64()).unwrap_or(0))
}

/// A new, unguessable id for a submission: one plugin cannot read another's answer by counting.
pub fn new_job_id() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub async fn create(
    db: &Db,
    job_id: &str,
    plugin: &str,
    callback: Option<&Callback>,
    requested_by: Option<&str>,
) -> Result<(), surrealdb::Error> {
    let callbacks: Vec<Value> = callback.map(|c| vec![c.to_json()]).unwrap_or_default();
    db.query(
        "CREATE type::record('ai_result', $id) CONTENT { job_id: $id, status: 'pending', plugin: $plugin, callbacks: $callbacks, requested_by: $by };",
    )
    .bind(("id", job_id.to_string()))
    .bind(("plugin", plugin.to_string()))
    .bind(("callbacks", callbacks))
    .bind(("by", requested_by.map(str::to_string)))
    .await?
    .check()?;
    Ok(())
}

async fn set_status(db: &Db, job_id: &str, status: &str, error: Option<&str>) -> Result<(), surrealdb::Error> {
    db.query("UPDATE ai_result SET status = $status, error = $error WHERE job_id = $id;")
        .bind(("status", status.to_string()))
        .bind(("error", error.map(|e| e.chars().take(500).collect::<String>())))
        .bind(("id", job_id.to_string()))
        .await?
        .check()?;
    Ok(())
}

/// A job that could not be queued must not leave a `pending` row that nothing will ever run.
pub async fn mark_failed(db: &Db, job_id: &str, message: &str) {
    let _ = set_status(db, job_id, "failed", Some(message)).await;
}

async fn store_done(db: &Db, job_id: &str, response: &ChatResponse) -> Result<(), surrealdb::Error> {
    db.query("UPDATE ai_result SET status = 'done', result = $result, error = NONE WHERE job_id = $id;")
        .bind(("result", serde_json::to_value(response).unwrap_or(Value::Null)))
        .bind(("id", job_id.to_string()))
        .await?
        .check()?;
    Ok(())
}

fn callbacks_of(row: &Value) -> Vec<Callback> {
    row["callbacks"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|c| {
                    Some(Callback {
                        plugin: c["plugin"].as_str()?.to_string(),
                        function: c["function"].as_str()?.to_string(),
                        reference: c["ref"].as_str().map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The payload a plugin's `on_done` function receives. The answer itself is fetched with `ai::result`,
/// so a job payload stays small.
pub fn callback_payload(row: &Value, callback: &Callback) -> Value {
    json!({
        "event": "ai.done", "job_id": row["job_id"], "status": row["status"], "error": row["error"],
        "usage": row["result"]["usage"], "ref": callback.reference,
    })
}

/// Queue the plugin functions that asked to be told. One failing to queue does not stop the others.
pub async fn fire_callbacks(db: &Db, row: &Value, state: &AppState, org_db: &str) {
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
    state.scheduler.wake(org_db);
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
                fire_callbacks(&org, &row, state, org_db).await;
            }
            Ok(())
        }
        Err((message, permanent)) => {
            if permanent || final_attempt {
                let _ = set_status(&org, &job_id, "failed", Some(&message)).await;
                if let Ok(Some(row)) = find(&org, &job_id).await {
                    fire_callbacks(&org, &row, state, org_db).await;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(model: Option<&str>) -> ChatRequest {
        ChatRequest {
            messages: vec![aether_ai::Message::new("user", "hi")],
            model: model.map(str::to_string),
            max_tokens: Some(5000),
            temperature: None,
            json: true,
            tools: vec![],
        }
    }

    #[test]
    fn the_provider_is_told_the_organizations_model_and_never_more_tokens_than_allowed() {
        let mut limits = Limits::default();
        limits.model = Some("llama3.2".into());
        let params = chat_params(&ask(None), &limits, 8);
        assert_eq!(params["model"], "llama3.2");
        assert_eq!(params["max_tokens"], 1024);
        assert_eq!(params["timeout_secs"], 8);
        assert_eq!(params["json"], true);
        // A request's own model wins.
        assert_eq!(chat_params(&ask(Some("other")), &limits, 8)["model"], "other");
    }

    #[test]
    fn a_provider_answer_of_another_shape_is_refused() {
        assert!(parse_response(json!({ "text": "x" })).is_err());
        let ok = parse_response(json!({ "text": "x", "model": "m", "provider": "p", "finish_reason": "stop", "usage": { "input_tokens": 1, "output_tokens": 2 } }));
        assert_eq!(ok.map(|r| r.usage.total()), Ok(3));
    }

    #[test]
    fn job_ids_are_long_and_do_not_repeat() {
        let (a, b) = (new_job_id(), new_job_id());
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }

    #[test]
    fn callbacks_get_a_small_payload_without_the_answer() {
        let row = json!({
            "job_id": "j", "status": "done", "error": null, "result": { "text": "secret answer", "usage": { "input_tokens": 1, "output_tokens": 2 } },
            "callbacks": [{ "plugin": "hr", "function": "answered", "ref": "r1" }, { "plugin": "x" }],
        });
        let callbacks = callbacks_of(&row);
        assert_eq!(callbacks.len(), 1);
        let payload = callback_payload(&row, &callbacks[0]);
        assert_eq!(payload["event"], "ai.done");
        assert_eq!(payload["ref"], "r1");
        assert!(!payload.to_string().contains("secret answer"));
    }
}
