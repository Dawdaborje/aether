//! `ai::complete`, `ai::embed`, `ai::status`, `ai::submit` and `ai::result`: a language model, through the kernel.
//!
//! ```json
//! { "prompt": "Summarise this note: ...", "system": "Answer in one sentence." }
//! { "messages": [{ "role": "user", "content": "..." }], "model": "llama3.2", "max_tokens": 300, "json": false }
//! ```
//!
//! The plugin never holds a key or an address: the provider (Ollama, an OpenAI-compatible server,
//! Anthropic) and its credentials are the organization's settings, read by the kernel. Needs the capability
//! `ai::complete` (`ai::embed` for embeddings).
//!
//! AI is **off** until an organization turns `ai.enabled` on, because a prompt leaves the plugin and, with
//! a remote provider, the organization's network. An organization may also allow only local providers
//! (`ai.local_only`) and cap the tokens used per day for itself and for each person. Every call adds a row
//! to the usage log (who, plugin, model, tokens, time), never the prompt or the answer.
//!
//! A plugin call may wait only a few seconds, so `ai::complete` and `ai::embed` give up after
//! `ai.sync_ceiling_secs`; a longer answer is for `ai::submit`, which queues a job and calls the plugin's own
//! function when it ends. A plugin reads only the answers to its own submissions.

use std::time::Instant;

use aether_ai::{ChatRequest, Limits, MAX_EMBED_INPUTS, Message, gate};
use serde::Deserialize;
use serde_json::{Value as JsonValue, json};

use super::context::PluginHostContext;
use super::error::HostError;
use crate::ai_jobs::{self, Callback, Usage};

async fn limits(ctx: &PluginHostContext) -> Limits {
    let values = super::settings_read::effective_settings(ctx, aether_ai::keys::ALL).await;
    Limits::from_settings(|key| values.get(key).cloned())
}

fn failure(error: aether_communication::SendError) -> HostError {
    HostError::Message(error.message)
}

/// The conversation of a payload: `messages`, or a `prompt` (and an optional `system`) as shorthand.
fn request_of(payload: &JsonValue) -> Result<ChatRequest, HostError> {
    let mut value = payload.clone();
    if value.get("messages").is_none() {
        let prompt = payload["prompt"].as_str().ok_or_else(|| HostError::InvalidPayload("give `messages`, or a `prompt`".into()))?;
        let mut messages = Vec::new();
        if let Some(system) = payload["system"].as_str().filter(|s| !s.trim().is_empty()) {
            messages.push(Message::new("system", system));
        }
        messages.push(Message::new("user", prompt));
        value["messages"] = serde_json::to_value(messages).unwrap_or_default();
    }
    serde_json::from_value(value).map_err(|error| HostError::InvalidPayload(error.to_string()))
}

/// Everything checked before a provider is called: on, allowed, and within today's budget.
async fn admit(ctx: &PluginHostContext, limits: &Limits) -> Result<&'static aether_ai::Provider, HostError> {
    if !limits.enabled {
        return Err(HostError::Message("AI is turned off for this organization".into()));
    }
    let provider = limits.chosen_provider().map_err(HostError::Message)?;
    ai_jobs::check_budget(&ctx.db, limits, ctx.audit.actor.id()).await.map_err(HostError::Message)?;
    Ok(provider)
}

fn handle(ctx: &PluginHostContext) -> Result<std::sync::Arc<dyn super::context::BridgeHandle>, HostError> {
    ctx.services()?.bridges.clone().ok_or_else(|| HostError::Message("AI is not available here".into()))
}

pub async fn ai_status(ctx: &PluginHostContext, _payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("ai::complete")?;
    let limits = limits(ctx).await;
    let mut data = json!({
        "enabled": limits.enabled, "provider": limits.provider, "model": limits.model, "embed_model": limits.embed_model,
        "local_only": limits.local_only,
        "limits": {
            "max_tokens": limits.max_tokens, "max_prompt_bytes": limits.max_prompt_bytes,
            "sync_ceiling_secs": limits.sync_ceiling_secs, "daily_tokens_org": limits.daily_tokens_org,
            "daily_tokens_user": limits.daily_tokens_user,
        },
    });
    if limits.enabled {
        match limits.chosen_provider() {
            Ok(provider) => {
                // Whether the provider answers, and what it offers; a failure is a state, not an error.
                let listed = handle(ctx)?.call_internal(&ctx.database, provider.key, "models", json!({})).await;
                match listed {
                    Ok(models) => {
                        data["reachable"] = json!(true);
                        data["models"] = models["models"].clone();
                    }
                    Err(error) => {
                        data["reachable"] = json!(false);
                        data["error"] = json!(error.message);
                    }
                }
            }
            Err(message) => data["error"] = json!(message),
        }
    }
    Ok(json!({ "ok": true, "data": data }))
}

pub async fn ai_complete(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("ai::complete")?;
    let limits = limits(ctx).await;
    let request = request_of(payload)?;
    request.validate(&limits).map_err(HostError::InvalidPayload)?;
    let provider = admit(ctx, &limits).await?;
    let bridges = handle(ctx)?;
    let permit = gate()
        .acquire(limits.max_concurrent, std::time::Duration::from_secs(2))
        .await
        .ok_or_else(|| HostError::Message("the AI provider is busy: try again in a moment".into()))?;
    let started = Instant::now();
    let params = ai_jobs::chat_params(&request, &limits, limits.sync_ceiling_secs);
    let ceiling = std::time::Duration::from_secs(limits.sync_ceiling_secs);
    // The bridge keeps the same limit for its request; this one also covers a provider that stalls.
    let answer = match tokio::time::timeout(ceiling, bridges.call_internal(&ctx.database, provider.key, "chat", params)).await {
        Ok(answer) => answer,
        Err(_) => Err(aether_communication::SendError::transient(format!(
            "the model did not answer within {} seconds: use ai::submit for a long answer",
            limits.sync_ceiling_secs
        ))),
    };
    drop(permit);
    let parsed = answer.map_err(|e| e.message).and_then(ai_jobs::parse_response);
    let (input, output) = parsed.as_ref().map(|r| (r.usage.input_tokens, r.usage.output_tokens)).unwrap_or((0, 0));
    ai_jobs::record_usage(
        &ctx.db,
        Usage {
            who: ctx.audit.actor.id(),
            plugin: Some(&ctx.plugin_name),
            kind: "complete",
            provider: provider.key,
            model: parsed.as_ref().ok().map(|r| r.model.as_str()),
            input_tokens: input,
            output_tokens: output,
            duration_ms: started.elapsed().as_millis() as u64,
            ok: parsed.is_ok(),
        },
    )
    .await;
    // Never the prompt or the answer.
    log::info!(
        "ai: plugin `{}` {} in {} out {} tokens in {} ms ({})",
        ctx.plugin_name,
        provider.key,
        input,
        output,
        started.elapsed().as_millis(),
        if parsed.is_ok() { "ok" } else { "failed" }
    );
    let response = parsed.map_err(HostError::Message)?;
    Ok(json!({ "ok": true, "data": response }))
}

#[derive(Deserialize)]
struct EmbedRequest {
    /// One text, or a list.
    input: JsonValue,
    #[serde(default)]
    model: Option<String>,
}

/// Embed `texts` with the organization's provider: checked, budgeted, bounded in time and logged. Answers the
/// vectors, the model that made them and the tokens the provider counted.
async fn embed_texts(
    ctx: &PluginHostContext,
    limits: &Limits,
    texts: &[String],
    model: Option<&str>,
) -> Result<(JsonValue, String, &'static str), HostError> {
    if texts.is_empty() || texts.len() > MAX_EMBED_INPUTS || texts.iter().any(|t| t.trim().is_empty()) {
        return Err(HostError::InvalidPayload(format!("`input` is 1 to {MAX_EMBED_INPUTS} texts, none empty")));
    }
    if texts.iter().map(String::len).sum::<usize>() > limits.max_prompt_bytes {
        return Err(HostError::InvalidPayload(format!("the texts are over {} KB", limits.max_prompt_bytes / 1024)));
    }
    if let Some(model) = model {
        aether_ai::valid_model(model).map_err(HostError::InvalidPayload)?;
    }
    let provider = admit(ctx, limits).await?;
    let bridges = handle(ctx)?;
    let permit = gate()
        .acquire(limits.max_concurrent, std::time::Duration::from_secs(2))
        .await
        .ok_or_else(|| HostError::Message("the AI provider is busy: try again in a moment".into()))?;
    let started = Instant::now();
    let params = json!({
        "input": texts, "model": model.map(str::to_string).or_else(|| limits.embed_model.clone()), "timeout_secs": limits.sync_ceiling_secs,
    });
    let ceiling = std::time::Duration::from_secs(limits.sync_ceiling_secs);
    let answer = match tokio::time::timeout(ceiling, bridges.call_internal(&ctx.database, provider.key, "embed", params)).await {
        Ok(answer) => answer,
        Err(_) => Err(aether_communication::SendError::transient("the model did not answer in time")),
    };
    drop(permit);
    let input_tokens = answer.as_ref().ok().and_then(|a| a["usage"]["input_tokens"].as_u64()).unwrap_or(0);
    let used_model = answer.as_ref().ok().and_then(|a| a["model"].as_str()).map(str::to_string);
    ai_jobs::record_usage(
        &ctx.db,
        Usage {
            who: ctx.audit.actor.id(),
            plugin: Some(&ctx.plugin_name),
            kind: "embed",
            provider: provider.key,
            model: used_model.as_deref(),
            input_tokens,
            output_tokens: 0,
            duration_ms: started.elapsed().as_millis() as u64,
            ok: answer.is_ok(),
        },
    )
    .await;
    let data = answer.map_err(failure)?;
    Ok((data, used_model.unwrap_or_default(), provider.key))
}

pub async fn ai_embed(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("ai::embed")?;
    let limits = limits(ctx).await;
    let request: EmbedRequest = serde_json::from_value(payload.clone()).map_err(|e| HostError::InvalidPayload(e.to_string()))?;
    let texts: Vec<String> = match &request.input {
        JsonValue::String(text) => vec![text.clone()],
        JsonValue::Array(list) => list
            .iter()
            .map(|t| t.as_str().map(str::to_string).ok_or_else(|| HostError::InvalidPayload("`input` is a text or a list of texts".into())))
            .collect::<Result<_, _>>()?,
        _ => return Err(HostError::InvalidPayload("`input` is a text or a list of texts".into())),
    };
    let (data, _, provider) = embed_texts(ctx, &limits, &texts, request.model.as_deref()).await?;
    Ok(json!({ "ok": true, "data": { "embeddings": data["embeddings"], "model": data["model"], "provider": provider, "usage": data["usage"] } }))
}

/// Most pieces one `ai::index` call embeds: a call may wait only a few seconds.
const MAX_INDEX_PIECES: usize = 16;
const CHUNK_BYTES: usize = 1000;
const CHUNK_OVERLAP: usize = 100;

fn source_of(payload: &JsonValue) -> Result<String, HostError> {
    payload["source"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.len() <= 200 && !s.chars().any(char::is_control))
        .map(str::to_string)
        .ok_or_else(|| HostError::InvalidPayload("`source` names what the text is from (at most 200 characters)".into()))
}

fn embeddings_of(data: &JsonValue) -> Vec<Vec<f64>> {
    data["embeddings"]
        .as_array()
        .map(|list| list.iter().map(|v| v.as_array().map(|n| n.iter().filter_map(JsonValue::as_f64).collect()).unwrap_or_default()).collect())
        .unwrap_or_default()
}

/// Remember `text` for search: `{ source, text, append? }`. The text is cut into pieces and embedded; the plugin's
/// earlier pieces of the same `source` are replaced unless `append` is set (to feed a long document in parts).
pub async fn ai_index(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("ai::embed")?;
    let limits = limits(ctx).await;
    let source = source_of(payload)?;
    let text = payload["text"].as_str().ok_or_else(|| HostError::InvalidPayload("`text` is required".into()))?;
    let pieces = aether_ai::chunk_text(text, CHUNK_BYTES, CHUNK_OVERLAP);
    if pieces.is_empty() {
        return Err(HostError::InvalidPayload("`text` is empty".into()));
    }
    if pieces.len() > MAX_INDEX_PIECES {
        return Err(HostError::InvalidPayload(format!(
            "`text` makes {} pieces and one call takes {MAX_INDEX_PIECES} (about {} KB): send a long document in parts with `append`",
            pieces.len(),
            MAX_INDEX_PIECES
        )));
    }
    let append = payload["append"].as_bool().unwrap_or(false);
    let (data, model, _) = embed_texts(ctx, &limits, &pieces, None).await?;
    let vectors = embeddings_of(&data);
    if vectors.len() != pieces.len() || vectors.iter().any(Vec::is_empty) {
        return Err(HostError::Message("the provider did not return one vector for each piece".into()));
    }
    let db_error = |error: surrealdb::Error| {
        log::error!("ai: index in {} {error}", ctx.database);
        HostError::Message("the text could not be stored".into())
    };
    let first = if append {
        let mut response = ctx
            .db
            .query("SELECT count() AS n FROM ai_chunk WHERE plugin = $plugin AND source = $source GROUP ALL;")
            .bind(("plugin", ctx.plugin_name.clone()))
            .bind(("source", source.clone()))
            .await
            .map_err(db_error)?;
        let rows: Vec<JsonValue> = response.take(0).map_err(db_error)?;
        rows.first().and_then(|r| r["n"].as_i64()).unwrap_or(0)
    } else {
        ctx.db
            .query("DELETE ai_chunk WHERE plugin = $plugin AND source = $source;")
            .bind(("plugin", ctx.plugin_name.clone()))
            .bind(("source", source.clone()))
            .await
            .map_err(db_error)?
            .check()
            .map_err(db_error)?;
        0
    };
    let rows: Vec<JsonValue> = pieces
        .iter()
        .zip(vectors)
        .enumerate()
        .map(|(i, (text, embedding))| json!({ "plugin": ctx.plugin_name, "source": source, "ord": first + i as i64, "text": text, "model": model, "embedding": embedding }))
        .collect();
    ctx.db.query("INSERT INTO ai_chunk $rows;").bind(("rows", rows)).await.map_err(db_error)?.check().map_err(db_error)?;
    Ok(json!({ "ok": true, "data": { "source": source, "pieces": pieces.len(), "model": model } }))
}

/// Find the pieces closest in meaning to a question: `{ query, k?, source_prefix? }` answers
/// `[{ source, ord, text, score }]`, best first, from this plugin's own indexed text.
pub async fn ai_search(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("ai::embed")?;
    let limits = limits(ctx).await;
    let query = payload["query"].as_str().filter(|q| !q.trim().is_empty()).ok_or_else(|| HostError::InvalidPayload("`query` is required".into()))?;
    let k = payload["k"].as_u64().unwrap_or(4).clamp(1, 10);
    let prefix = payload["source_prefix"].as_str().filter(|p| !p.is_empty() && p.len() <= 200).map(str::to_string);
    let (data, model, _) = embed_texts(ctx, &limits, &[query.to_string()], None).await?;
    let vector = embeddings_of(&data).into_iter().next().filter(|v| !v.is_empty()).ok_or_else(|| HostError::Message("the provider returned no vector".into()))?;
    let filter = if prefix.is_some() { " AND string::starts_with(source, $prefix)" } else { "" };
    let sql = format!(
        "SELECT source, ord, text, vector::similarity::cosine(embedding, $vector) AS score FROM ai_chunk \
         WHERE plugin = $plugin AND model = $model{filter} ORDER BY score DESC LIMIT $k;"
    );
    let db_error = |error: surrealdb::Error| {
        log::error!("ai: search in {} {error}", ctx.database);
        HostError::Message("the text could not be searched".into())
    };
    let mut response = ctx
        .db
        .query(sql)
        .bind(("vector", vector))
        .bind(("plugin", ctx.plugin_name.clone()))
        .bind(("model", model))
        .bind(("prefix", prefix))
        .bind(("k", k as i64))
        .await
        .map_err(db_error)?;
    let rows: Vec<JsonValue> = response.take(0).map_err(db_error)?;
    Ok(json!({ "ok": true, "data": rows }))
}

/// Forget what `ai::index` kept for a `source`: `{ source }` answers `{ forgotten: true }`.
pub async fn ai_forget(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("ai::embed")?;
    let source = source_of(payload)?;
    ctx.db
        .query("DELETE ai_chunk WHERE plugin = $plugin AND source = $source;")
        .bind(("plugin", ctx.plugin_name.clone()))
        .bind(("source", source))
        .await
        .and_then(|r| r.check())
        .map_err(|error| {
            log::error!("ai: forget in {} {error}", ctx.database);
            HostError::Message("the text could not be removed".into())
        })?;
    Ok(json!({ "ok": true, "data": { "forgotten": true } }))
}

fn is_function_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Queue a conversation that may take longer than a call can wait. Answers at once with `{ job_id, status }`.
pub async fn ai_submit(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("ai::complete")?;
    let limits = limits(ctx).await;
    let request = request_of(payload)?;
    request.validate(&limits).map_err(HostError::InvalidPayload)?;
    let reference = payload["ref"].as_str().map(str::to_string);
    if reference.as_ref().is_some_and(|r| r.len() > 200) {
        return Err(HostError::InvalidPayload("`ref` is at most 200 characters".into()));
    }
    let callback = match payload["on_done"].as_str() {
        Some(function) if is_function_name(function) => {
            Some(Callback { plugin: ctx.plugin_name.clone(), function: function.to_string(), reference })
        }
        Some(_) => return Err(HostError::InvalidPayload("`on_done` names one of this plugin's functions".into())),
        None => None,
    };
    admit(ctx, &limits).await?;
    let db_error = |error: surrealdb::Error| {
        log::error!("ai: {} {error}", ctx.database);
        HostError::Message("the request could not be recorded".into())
    };
    if ai_jobs::in_flight(&ctx.db).await.map_err(db_error)? >= limits.max_queued {
        return Err(HostError::Message(format!("{} requests are already waiting: try again later", limits.max_queued)));
    }
    let who = ctx.audit.actor.id().map(str::to_string);
    let job_id = ai_jobs::new_job_id();
    ai_jobs::create(&ctx.db, &job_id, &ctx.plugin_name, callback.as_ref(), who.as_deref()).await.map_err(db_error)?;
    let mut job = crate::scheduler::queue::NewJob::new(ai_jobs::KIND, ai_jobs::QUEUE);
    job.plugin = Some(ctx.plugin_name.clone());
    job.payload = Some(json!({ "job_id": job_id, "request": request, "by": who }));
    job.max_attempts = 2;
    job.unique_key = Some(job_id.clone());
    job.enqueued_by = who;
    job.request_id = Some(ctx.audit.request_id.clone());
    if let Err(error) = crate::scheduler::queue::enqueue(&ctx.db, job).await {
        ai_jobs::mark_failed(&ctx.db, &job_id, "the job could not be queued").await;
        return Err(HostError::Message(format!("the request could not be queued ({error})")));
    }
    if let Some(scheduler) = ctx.services()?.scheduler.clone() {
        scheduler.wake(&ctx.database);
    }
    Ok(json!({ "ok": true, "data": { "job_id": job_id, "status": "pending" } }))
}

/// The state, and when done the answer, of a submission. Only the plugin that submitted it can read it.
pub async fn ai_result(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("ai::complete")?;
    let job_id = payload["job_id"]
        .as_str()
        .filter(|id| id.len() == 32 && id.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| HostError::InvalidPayload("`job_id` is what `ai::submit` returned".into()))?;
    let row = ai_jobs::find(&ctx.db, job_id).await.map_err(|error| {
        log::error!("ai: {} {error}", ctx.database);
        HostError::Message("the result could not be read".into())
    })?;
    let data = match row {
        Some(row) if row["plugin"].as_str() == Some(ctx.plugin_name.as_str()) => json!({
            "status": row["status"], "job_id": row["job_id"], "result": row["result"], "error": row["error"], "updated": row["date_updated"],
        }),
        // Another plugin's job is reported as absent: its existence is not the caller's business.
        _ => json!({ "status": "none", "job_id": job_id }),
    };
    Ok(json!({ "ok": true, "data": data }))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use aether_communication::SendError;

    use super::*;
    use crate::kernel::host::{
        context::BridgeHandle,
        dispatch::kernel_command,
        test_support::{dummy_ctx, in_memory_media, with_services},
    };

    /// Stands in for the provider: records what it was asked and answers a fixed reply.
    struct Provider(Mutex<Vec<(String, String, JsonValue)>>);

    #[async_trait::async_trait]
    impl BridgeHandle for Provider {
        async fn call(&self, _: &str, _: &str, _: &str, _: JsonValue) -> Result<JsonValue, SendError> {
            Err(SendError::permanent("not a plugin bridge"))
        }

        async fn call_internal(&self, _org: &str, bridge: &str, action: &str, params: JsonValue) -> Result<JsonValue, SendError> {
            if let Ok(mut seen) = self.0.lock() {
                seen.push((bridge.into(), action.into(), params));
            }
            Ok(match action {
                "chat" => json!({ "text": "hello", "model": "m", "provider": bridge, "finish_reason": "stop",
                                  "usage": { "input_tokens": 3, "output_tokens": 1 } }),
                "embed" => json!({ "embeddings": [[0.5, 0.5]], "model": "e", "usage": { "input_tokens": 2, "output_tokens": 0 } }),
                _ => json!({ "models": ["m"] }),
            })
        }
    }

    async fn context(caps: &[&str]) -> (PluginHostContext, Arc<Provider>) {
        let provider = Arc::new(Provider(Mutex::new(Vec::new())));
        let mut ctx = with_services(dummy_ctx(caps), in_memory_media());
        ctx.database = "org_acme".into();
        if let Some(services) = ctx.services.as_mut() {
            services.bridges = Some(provider.clone());
        }
        (ctx, provider)
    }

    #[tokio::test]
    async fn ai_is_off_by_default_and_the_capability_is_checked_first() {
        let (ctx, provider) = context(&[]).await;
        let denied = kernel_command(&ctx, "ai::complete", json!({ "prompt": "hi" })).await;
        assert!(matches!(denied, Err(HostError::Capability(_))), "{denied:?}");
        let (ctx, provider2) = context(&["ai::complete"]).await;
        let off = kernel_command(&ctx, "ai::complete", json!({ "prompt": "hi" })).await.unwrap_err();
        assert!(off.to_string().contains("turned off"), "{off}");
        // Nothing reached a provider.
        assert!(provider.0.lock().map(|s| s.is_empty()).unwrap_or(false));
        assert!(provider2.0.lock().map(|s| s.is_empty()).unwrap_or(false));
    }

    #[tokio::test]
    async fn a_prompt_becomes_messages_and_mistakes_are_refused() {
        let request = request_of(&json!({ "prompt": "Hi", "system": "Be brief.", "max_tokens": 10 })).unwrap();
        assert_eq!(request.messages.len(), 2);
        assert_eq!(request.messages[0].role, "system");
        assert!(request_of(&json!({})).is_err());
        assert!(request_of(&json!({ "messages": [{ "role": "user" }] })).is_err());
    }

    #[test]
    fn a_source_is_a_short_plain_name_and_vectors_are_read_defensively() {
        assert_eq!(source_of(&json!({ "source": " policy/leave.pdf " })).unwrap(), "policy/leave.pdf");
        for bad in [json!({}), json!({ "source": "" }), json!({ "source": "a\nb" }), json!({ "source": "x".repeat(201) })] {
            assert!(source_of(&bad).is_err(), "{bad}");
        }
        assert_eq!(embeddings_of(&json!({ "embeddings": [[1.0, 2.0], "junk"] })), vec![vec![1.0, 2.0], vec![]]);
    }

    #[tokio::test]
    async fn indexing_and_searching_need_the_embed_capability_and_a_enabled_ai() {
        let (ctx, provider) = context(&["ai::complete"]).await;
        for command in ["ai::index", "ai::search", "ai::forget"] {
            let denied = kernel_command(&ctx, command, json!({ "source": "s", "text": "t", "query": "q" })).await;
            assert!(matches!(denied, Err(HostError::Capability(_))), "{command}: {denied:?}");
        }
        let (ctx, provider2) = context(&["ai::embed"]).await;
        let off = kernel_command(&ctx, "ai::index", json!({ "source": "s", "text": "A policy." })).await.unwrap_err();
        assert!(off.to_string().contains("turned off"), "{off}");
        let big = kernel_command(&ctx, "ai::index", json!({ "source": "s", "text": "word ".repeat(10_000) })).await.unwrap_err();
        assert!(big.to_string().contains("in parts"), "{big}");
        assert!(provider.0.lock().unwrap().is_empty() && provider2.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn results_of_another_plugins_job_are_not_visible() {
        let (ctx, _) = context(&["ai::complete"]).await;
        let refused = kernel_command(&ctx, "ai::result", json!({ "job_id": "../x" })).await.unwrap_err();
        assert!(refused.to_string().contains("ai::submit"), "{refused}");
    }
}
