//! `ocr::parse` and `ocr::status`: read the text out of a file in the organization's media.
//!
//! ```json
//! { "media_key": "media/2026/10/ab12.pdf", "language": "eng", "pages": "1-3", "ocr": true, "name": "cv.pdf" }
//! ```
//!
//! The plugin names a stored file and never holds the engine's settings. The kernel reads the bytes
//! from whatever storage the organization uses, writes them to a private work folder for the
//! engine (a separate process, see the `liteparse` bridge), and answers the engine-independent
//! result of [`aether_ocr::OcrResult`]. Needs the capability `ocr::parse`.
//!
//! Every limit is a setting (`ocr.*`, global with an organization override, see `seeds/settings/ocr.json`):
//! whether parsing is on, the largest file, the most pages, the most text kept, how long a plugin call
//! waits, how many parses run at once. None can pass the hard caps in [`aether_ocr`]. A plugin call may
//! only wait a few seconds, so a file that takes longer than the ceiling is refused; long files are for
//! the job path.

use std::time::Instant;

use aether_ocr::{Limits, Options, TempInput, cache_key, extension_for, gate, keys};
use aether_storage::{MediaKey, StorageError};
use serde::Deserialize;
use serde_json::{Value as JsonValue, json};

use super::context::PluginHostContext;
use super::error::HostError;
use crate::data_model::media::is_media_key;

/// The engine a request uses; one exists so far.

/// The limits in force for the call's organization.
async fn limits(ctx: &PluginHostContext) -> Limits {
    let values = super::settings_read::effective_settings(ctx, keys::ALL).await;
    Limits::from_settings(|key| values.get(key).cloned())
}

use crate::ocr_jobs::{self, Callback, engine_of as engine};

#[derive(Deserialize)]
struct ParseRequest {
    #[serde(default)]
    media_key: Option<String>,
    /// A file in the calling plugin's own storage, instead of an uploaded one.
    #[serde(default)]
    storage_key: Option<String>,
    /// The file's name, when the key's own extension says nothing (an Office file).
    #[serde(default)]
    name: Option<String>,
    #[serde(flatten)]
    options: Options,
}

/// The file a request names: `media_key` (an uploaded file) or `storage_key` (a file in the plugin's own storage,
/// where it keeps receipts and the like). Answers the key text the result is cached under, and the key itself.
fn target(ctx: &PluginHostContext, media_key: Option<&str>, storage_key: Option<&str>) -> Result<(String, MediaKey), HostError> {
    match (media_key, storage_key) {
        (Some(_), Some(_)) => Err(HostError::InvalidPayload("give `media_key` or `storage_key`, not both".into())),
        (Some(media), None) => {
            if !is_media_key(media) {
                return Err(HostError::InvalidPayload("`media_key` is the key of an uploaded file".into()));
            }
            let key = MediaKey::parse(media).map_err(|error| HostError::InvalidPayload(error.to_string()))?;
            Ok((media.to_string(), key))
        }
        (None, Some(stored)) => {
            let key = super::storage::plugin_key(ctx, stored)?;
            Ok((key.as_str().to_string(), key))
        }
        (None, None) => Err(HostError::InvalidPayload("give `media_key` (an uploaded file) or `storage_key` (one of the plugin's stored files)".into())),
    }
}

fn failure(error: aether_communication::SendError) -> HostError {
    HostError::Message(error.message)
}

pub async fn ocr_status(ctx: &PluginHostContext, _payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("ocr::parse")?;
    let limits = limits(ctx).await;
    let handle = ctx
        .services()?
        .bridges
        .clone()
        .ok_or_else(|| HostError::Message("document parsing is not available here".into()))?;
    let mut answer = handle.call_internal(&ctx.database, engine(&limits), "status", json!({})).await.map_err(failure)?;
    // What the organization has set, beside what the engine reports.
    answer["enabled"] = json!(limits.enabled);
    answer["limits"] = json!({
        "max_input_bytes": limits.max_input_bytes, "max_pages": limits.max_pages, "max_text_bytes": limits.max_text_bytes,
        "sync_ceiling_secs": limits.sync_ceiling_secs, "max_concurrent": limits.max_concurrent,
    });
    Ok(json!({ "ok": true, "data": answer }))
}

pub async fn ocr_parse(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("ocr::parse")?;
    let limits = limits(ctx).await;
    if !limits.enabled {
        return Err(HostError::Message("document parsing is turned off for this organization".into()));
    }
    let request: ParseRequest =
        serde_json::from_value(payload.clone()).map_err(|error| HostError::InvalidPayload(error.to_string()))?;
    request.options.validate().map_err(HostError::InvalidPayload)?;
    let (media_text, key) = target(ctx, request.media_key.as_deref(), request.storage_key.as_deref())?;
    let services = ctx.services()?;
    let handle = services
        .bridges
        .clone()
        .ok_or_else(|| HostError::Message("document parsing is not available here".into()))?;
    // Parsed before (by anyone, in this organization)? The same file with the same options is not read twice.
    let key_text = cache_key(&media_text, engine(&limits), &request.options);
    if let Ok(Some(row)) = ocr_jobs::find(&ctx.db, &key_text).await {
        if row["status"] == "done" {
            if let Some(mut result) = row.get("result").cloned().filter(|r| r.is_object()) {
                result["cached"] = json!(true);
                return Ok(json!({ "ok": true, "data": result }));
            }
        }
    }
    // The size first, so a file over the limit is refused without being read into memory.
    match services.media.head(&key).await {
        Ok(object) if object.size as usize > limits.max_input_bytes => {
            return Err(HostError::Message(format!("files over {} MB are not parsed", limits.max_input_bytes / (1024 * 1024))));
        }
        Ok(_) => {}
        Err(StorageError::NotFound(_)) => return Err(HostError::Message("that file does not exist".into())),
        Err(error) => {
            log::error!("ocr: storage in `{}`: {error}", ctx.database);
            return Err(HostError::Message("storage is unavailable".into()));
        }
    }
    let bytes = match services.media.get(&key).await {
        Ok(bytes) => bytes,
        Err(StorageError::NotFound(_)) => return Err(HostError::Message("that file does not exist".into())),
        Err(error) => {
            log::error!("ocr: storage in `{}`: {error}", ctx.database);
            return Err(HostError::Message("storage is unavailable".into()));
        }
    };
    if bytes.len() > limits.max_input_bytes {
        return Err(HostError::Message(format!("files over {} MB are not parsed", limits.max_input_bytes / (1024 * 1024))));
    }
    let extension = extension_for(&bytes, request.name.as_deref())
        .ok_or_else(|| HostError::Message("this kind of file is not parsed".into()))?;
    // Two parses at a time: a parse is heavy, and a caller that finds the engine busy retries.
    let permit = gate()
        .acquire(limits.max_concurrent, std::time::Duration::from_secs(2))
        .await
        .ok_or_else(|| HostError::Message("the document reader is busy: try again in a moment".into()))?;
    let input = TempInput::create(&bytes, extension).map_err(|error| {
        log::error!("ocr: work folder: {error}");
        HostError::Message("the document reader has no room to work".into())
    })?;
    let started = Instant::now();
    let answer = handle
        .call_internal(
            &ctx.database,
            engine(&limits),
            "parse",
            json!({
                "path": input.path(), "language": request.options.language, "pages": request.options.pages,
                "ocr": request.options.ocr, "timeout_secs": limits.sync_ceiling_secs,
                "max_pages": limits.max_pages, "max_text_bytes": limits.max_text_bytes,
            }),
        )
        .await;
    drop(permit);
    // Never the text or the file's name: scans hold people's papers.
    log::info!(
        "ocr: plugin `{}` {extension} {} bytes in {} ms ({})",
        ctx.plugin_name,
        bytes.len(),
        started.elapsed().as_millis(),
        if answer.is_ok() { "ok" } else { "failed" }
    );
    if let Ok(data) = &answer {
        if let Err(error) = ocr_jobs::store_done(&ctx.db, &key_text, &media_text, engine(&limits), &request.options, data).await {
            log::warn!("ocr: result of a parse could not be kept in `{}`: {error}", ctx.database);
        }
    }
    answer.map(|data| json!({ "ok": true, "data": data })).map_err(failure)
}

#[derive(Deserialize)]
struct SubmitRequest {
    #[serde(default)]
    media_key: Option<String>,
    #[serde(default)]
    storage_key: Option<String>,
    #[serde(default)]
    name: Option<String>,
    /// One of the calling plugin's own functions, run when the parse ends with `{ event, cache_key,
    /// media_key, status, error, pages, ocr_used, ref }`. Fetch the text with `ocr::result`.
    #[serde(default)]
    on_done: Option<String>,
    /// Returned to `on_done` unchanged: say which record the file belongs to.
    #[serde(default, rename = "ref")]
    reference: Option<String>,
    #[serde(flatten)]
    options: Options,
}

fn is_function_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Queue a parse that may take longer than a call can wait. Answers at once with the state of the result.
pub async fn ocr_submit(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("ocr::parse")?;
    let limits = limits(ctx).await;
    if !limits.enabled {
        return Err(HostError::Message("document parsing is turned off for this organization".into()));
    }
    let request: SubmitRequest =
        serde_json::from_value(payload.clone()).map_err(|error| HostError::InvalidPayload(error.to_string()))?;
    request.options.validate().map_err(HostError::InvalidPayload)?;
    let (media_text, key) = target(ctx, request.media_key.as_deref(), request.storage_key.as_deref())?;
    if request.reference.as_ref().is_some_and(|r| r.len() > 200) {
        return Err(HostError::InvalidPayload("`ref` is at most 200 characters".into()));
    }
    let callback = match &request.on_done {
        Some(function) if is_function_name(function) => Some(Callback {
            plugin: ctx.plugin_name.clone(),
            function: function.clone(),
            reference: request.reference.clone(),
        }),
        Some(_) => return Err(HostError::InvalidPayload("`on_done` names one of this plugin's functions".into())),
        None => None,
    };
    let services = ctx.services()?;
    match services.media.head(&key).await {
        Ok(object) if object.size as usize > limits.max_input_bytes => {
            return Err(HostError::Message(format!("files over {} MB are not parsed", limits.max_input_bytes / (1024 * 1024))));
        }
        Ok(_) => {}
        Err(StorageError::NotFound(_)) => return Err(HostError::Message("that file does not exist".into())),
        Err(error) => {
            log::error!("ocr: storage in `{}`: {error}", ctx.database);
            return Err(HostError::Message("storage is unavailable".into()));
        }
    }
    let engine = engine(&limits);
    let key_text = cache_key(&media_text, engine, &request.options);
    let db_error = |error: surrealdb::Error| {
        log::error!("ocr: {} {error}", ctx.database);
        HostError::Message("the result could not be recorded".into())
    };
    let known = ocr_jobs::find(&ctx.db, &key_text).await.map_err(db_error)?;
    let waiting = matches!(known.as_ref().and_then(|r| r["status"].as_str()), Some("pending" | "running" | "done"));
    if !waiting && ocr_jobs::in_flight(&ctx.db).await.map_err(db_error)? >= limits.max_queued {
        return Err(HostError::Message(format!("{} parses are already waiting: try again later", limits.max_queued)));
    }
    let who = ctx.audit.actor.id().map(str::to_string);
    let needs_job = ocr_jobs::request(&ctx.db, &key_text, &media_text, engine, &request.options, callback.as_ref(), who.as_deref())
        .await
        .map_err(db_error)?;
    let scheduler = services.scheduler.clone();
    if needs_job {
        let mut job = crate::scheduler::queue::NewJob::new(ocr_jobs::KIND, ocr_jobs::QUEUE);
        job.plugin = Some(ctx.plugin_name.clone());
        job.payload = Some(json!({
            "cache_key": key_text, "media_key": media_text, "name": request.name, "options": request.options,
        }));
        job.max_attempts = 2;
        job.unique_key = Some(key_text.clone());
        job.enqueued_by = who;
        job.request_id = Some(ctx.audit.request_id.clone());
        if let Err(error) = crate::scheduler::queue::enqueue(&ctx.db, job).await {
            ocr_jobs::mark_failed(&ctx.db, &key_text, "the job could not be queued").await;
            return Err(HostError::Message(format!("the parse could not be queued ({error})")));
        }
    } else if let (Some(callback), Some(row)) = (&callback, &known) {
        // Already done: tell the caller now, as it would have been told later.
        if row["status"] == "done" {
            let mut job = crate::scheduler::queue::NewJob::new("plugin", "default");
            job.plugin = Some(callback.plugin.clone());
            job.function = Some(callback.function.clone());
            job.payload = Some(ocr_jobs::callback_payload(row, callback));
            job.enqueued_by = Some("system:ocr".into());
            let _ = crate::scheduler::queue::enqueue(&ctx.db, job).await;
        }
    }
    if let Some(scheduler) = scheduler {
        scheduler.wake(&ctx.database);
    }
    let status = known.as_ref().and_then(|r| r["status"].as_str()).filter(|s| *s != "failed").unwrap_or("pending");
    Ok(json!({ "ok": true, "data": { "status": status, "cache_key": key_text, "cached": status == "done" } }))
}

#[derive(Deserialize)]
struct ResultRequest {
    #[serde(default)]
    cache_key: Option<String>,
    #[serde(default)]
    media_key: Option<String>,
    #[serde(default)]
    storage_key: Option<String>,
    #[serde(flatten)]
    options: Options,
}

/// The state, and when done the result, of a parse: by `cache_key`, or by `media_key` and the options.
pub async fn ocr_result(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("ocr::parse")?;
    let request: ResultRequest =
        serde_json::from_value(payload.clone()).map_err(|error| HostError::InvalidPayload(error.to_string()))?;
    let key_text = match (&request.cache_key, &request.media_key) {
        (Some(key), _) if key.len() == 64 && key.chars().all(|c| c.is_ascii_hexdigit()) => key.clone(),
        (Some(_), _) => return Err(HostError::InvalidPayload("`cache_key` is what `ocr::submit` returned".into())),
        (None, _) if request.media_key.is_some() || request.storage_key.is_some() => {
            request.options.validate().map_err(HostError::InvalidPayload)?;
            let (media_text, _) = target(ctx, request.media_key.as_deref(), request.storage_key.as_deref())?;
            let limits = limits(ctx).await;
            cache_key(&media_text, engine(&limits), &request.options)
        }
        (None, _) => return Err(HostError::InvalidPayload("give `cache_key`, `media_key` or `storage_key`".into())),
    };
    let row = ocr_jobs::find(&ctx.db, &key_text).await.map_err(|error| {
        log::error!("ocr: {} {error}", ctx.database);
        HostError::Message("the result could not be read".into())
    })?;
    let data = match row {
        None => json!({ "status": "none", "cache_key": key_text }),
        Some(row) => json!({
            "status": row["status"], "cache_key": row["cache_key"], "media_key": row["media_key"], "engine": row["engine"],
            "result": row["result"], "error": row["error"], "pages": row["pages"], "ocr_used": row["ocr_used"],
            "updated": row["date_updated"],
        }),
    };
    Ok(json!({ "ok": true, "data": data }))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use aether_communication::SendError;
    use bytes::Bytes;

    use super::*;
    use crate::kernel::host::{
        context::BridgeHandle,
        dispatch::kernel_command,
        test_support::{dummy_ctx, in_memory_media, with_services},
    };

    /// Stands in for the engine: records what it was asked and answers a fixed result.
    struct Engine(Mutex<Vec<(String, String, JsonValue)>>);

    #[async_trait::async_trait]
    impl BridgeHandle for Engine {
        async fn call(&self, _: &str, _: &str, _: &str, _: JsonValue) -> Result<JsonValue, SendError> {
            Err(SendError::permanent("not a plugin bridge"))
        }

        async fn call_internal(&self, _org: &str, bridge: &str, action: &str, params: JsonValue) -> Result<JsonValue, SendError> {
            // The file must be there while the engine runs.
            if action == "parse" {
                let path = params["path"].as_str().unwrap_or_default();
                assert!(std::path::Path::new(path).exists(), "the engine's file is missing");
            }
            if let Ok(mut seen) = self.0.lock() {
                seen.push((bridge.into(), action.into(), params));
            }
            Ok(json!({ "text": "hello", "pages": [], "total_pages": 1, "language": "eng", "engine": "liteparse",
                       "ocr_used": false, "truncated": false, "warnings": [] }))
        }
    }

    async fn context(caps: &[&str], files: &[(&str, &[u8])]) -> (PluginHostContext, Arc<Engine>) {
        let media = in_memory_media();
        for (key, data) in files {
            media.put(&MediaKey::parse(key).unwrap(), Bytes::copy_from_slice(data)).await.unwrap();
        }
        let engine = Arc::new(Engine(Mutex::new(Vec::new())));
        let mut ctx = with_services(dummy_ctx(caps), media);
        ctx.database = "org_acme".into();
        if let Some(services) = ctx.services.as_mut() {
            services.bridges = Some(engine.clone());
        }
        (ctx, engine)
    }

    #[tokio::test]
    async fn a_stored_file_is_handed_to_the_engine_with_the_limits_in_force() -> Result<(), HostError> {
        let (ctx, engine) = context(&["ocr::parse"], &[("media/2026/10/a.pdf", b"%PDF-1.4 hello")]).await;
        let answer = kernel_command(&ctx, "ocr::parse", json!({ "media_key": "media/2026/10/a.pdf", "language": "eng" })).await?;
        assert_eq!(answer["data"]["text"], "hello");
        let seen = engine.0.lock().map(|s| s.clone()).unwrap_or_default();
        assert_eq!((seen[0].0.as_str(), seen[0].1.as_str()), ("liteparse", "parse"));
        // Defaults (no settings in the test organization): the numbers in `seeds/settings/ocr.json`.
        assert_eq!(seen[0].2["max_pages"], 100);
        assert_eq!(seen[0].2["timeout_secs"], 8);
        assert_eq!(seen[0].2["max_text_bytes"], 2048 * 1024);
        assert!(seen[0].2["path"].as_str().is_some_and(|p| p.contains("aether-ocr")));
        Ok(())
    }

    #[tokio::test]
    async fn a_file_in_the_plugins_own_storage_can_be_named_and_nothing_else_of_storage() {
        let probe = dummy_ctx(&[]);
        let folder = format!("plugins/{}/receipts/1.pdf", probe.plugin_name);
        let (ctx, engine) = context(&["ocr::parse"], &[(folder.as_str(), b"%PDF-1.4 receipt")]).await;
        let answer = kernel_command(&ctx, "ocr::parse", json!({ "storage_key": "receipts/1.pdf" })).await.unwrap();
        assert_eq!(answer["data"]["text"], "hello");
        assert_eq!(engine.0.lock().unwrap().len(), 1);
        // The key is relative to the plugin's folder: it cannot climb out, and cannot be given with a media key.
        for payload in [
            json!({ "storage_key": "../other/x.pdf" }),
            json!({ "storage_key": "receipts/1.pdf", "media_key": "media/2026/10/a.pdf" }),
            json!({}),
        ] {
            assert!(kernel_command(&ctx, "ocr::parse", payload).await.is_err());
        }
        let missing = kernel_command(&ctx, "ocr::parse", json!({ "storage_key": "receipts/none.pdf" })).await.unwrap_err();
        assert!(missing.to_string().contains("does not exist"), "{missing}");
    }

    #[tokio::test]
    async fn the_capability_the_key_the_file_and_the_kind_are_checked() {
        let (ctx, _) = context(&[], &[("media/2026/10/a.pdf", b"%PDF-1.4")]).await;
        let denied = kernel_command(&ctx, "ocr::parse", json!({ "media_key": "media/2026/10/a.pdf" })).await;
        assert!(matches!(denied, Err(HostError::Capability(_))), "{denied:?}");

        let (ctx, _) = context(&["ocr::parse"], &[("media/2026/10/x.bin", b"\x00\x01\x02 not a document")]).await;
        for (payload, wanted) in [
            (json!({ "media_key": "../etc/passwd" }), "uploaded file"),
            (json!({ "media_key": "media/2026/10/missing.pdf" }), "does not exist"),
            (json!({ "media_key": "media/2026/10/x.bin" }), "not parsed"),
            (json!({ "media_key": "media/2026/10/x.bin", "language": "eng; rm -rf" }), "Tesseract"),
        ] {
            let refused = kernel_command(&ctx, "ocr::parse", payload).await.unwrap_err();
            assert!(refused.to_string().contains(wanted), "{refused}: wanted `{wanted}`");
        }
    }
}
