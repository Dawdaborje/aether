//! The job runner for `ocr_jobs`. The rows and their queries are in `aether_kernel::ocr_jobs`.

use aether_kernel::ocr_jobs::*;
pub use aether_kernel::ocr_jobs::*;

use std::time::Duration;
use aether_ocr::{Limits, OcrResult, Options, TempInput, extension_for, gate};
use aether_storage::{MediaKey, StorageError};
use serde_json::{Value, json};
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
        job.enqueued_by = Some("system:ocr".into());
        if let Err(error) = crate::scheduler::queue::enqueue(db, job).await {
            log::warn!("{org_db}: ocr callback to `{}.{}` could not be queued: {error}", callback.plugin, callback.function);
        }
    }
    scheduler.wake(org_db);
}

/// What the worker needs to say about a failed attempt: the message, and whether trying again cannot help.
pub type Failure = (String, bool);

/// Run one `ocr` job.
pub async fn run(state: &AppState, org_db: &str, job: &Job) -> Result<(), Failure> {
    let payload = job.payload.clone().unwrap_or(Value::Null);
    let cache_key = payload["cache_key"].as_str().ok_or(("the job names no result".to_string(), true))?.to_string();
    let media_key = payload["media_key"].as_str().ok_or(("the job names no file".to_string(), true))?.to_string();
    let name = payload["name"].as_str().map(str::to_string);
    let options: Options = serde_json::from_value(payload["options"].clone()).map_err(|e| (format!("the options: {e}"), true))?;
    let org = state.org(org_db).await.map_err(|e| (format!("the database: {e}"), false))?;
    let core = state.core().await.ok();
    let limits = limits_for(&org, core.as_ref()).await;
    let engine = engine_of(&limits);
    let _ = set_status(&org, &cache_key, "running", None).await;
    let final_attempt = job.attempts >= job.max_attempts;

    let outcome = parse_file(state, org_db, &media_key, name.as_deref(), &options, &limits, engine).await;
    match outcome {
        Ok(result) => {
            store_done(&org, &cache_key, &media_key, engine, &options, &result).await.map_err(|e| (format!("the result could not be kept: {e}"), false))?;
            if let Ok(Some(row)) = find(&org, &cache_key).await {
                fire_callbacks(&org, &row, &state.scheduler, org_db).await;
            }
            Ok(())
        }
        Err((message, permanent)) => {
            if permanent || final_attempt {
                let _ = set_status(&org, &cache_key, "failed", Some(&message)).await;
                if let Ok(Some(row)) = find(&org, &cache_key).await {
                    fire_callbacks(&org, &row, &state.scheduler, org_db).await;
                }
            } else {
                // The queue tries again; the row says it is waiting.
                let _ = set_status(&org, &cache_key, "pending", Some(&message)).await;
            }
            Err((message, permanent))
        }
    }
}

/// Read a stored file for a signed-in member (the document viewer's "Extracted text"): the kept result when this file was
/// parsed before, otherwise a parse now, kept for the next time. `Err` is a status and a message safe to show.
pub async fn read_for_member(
    state: &AppState,
    org_db: &str,
    media_key: &str,
    options: &Options,
) -> Result<Value, (u16, String)> {
    let org = state.org(org_db).await.map_err(|_| (500, "database error".to_string()))?;
    let core = state.core().await.ok();
    let limits = limits_for(&org, core.as_ref()).await;
    if !limits.enabled {
        return Err((403, "document parsing is turned off for this organization".into()));
    }
    let engine = engine_of(&limits);
    let cache = aether_ocr::cache_key(media_key, engine, options);
    if let Ok(Some(row)) = find(&org, &cache).await {
        if row["status"] == "done" {
            if let Some(mut result) = row.get("result").cloned().filter(Value::is_object) {
                result["cached"] = json!(true);
                return Ok(result);
            }
        }
    }
    let result = parse_file(state, org_db, media_key, None, options, &limits, engine)
        .await
        .map_err(|(message, permanent)| (if permanent { 422 } else { 503 }, message))?;
    if let Err(error) = store_done(&org, &cache, media_key, engine, options, &result).await {
        log::warn!("ocr: result of a parse could not be kept in `{org_db}`: {error}");
    }
    Ok(result)
}

async fn parse_file(
    state: &AppState,
    org_db: &str,
    media_key: &str,
    name: Option<&str>,
    options: &Options,
    limits: &Limits,
    engine: &str,
) -> Result<Value, Failure> {
    if !limits.enabled {
        return Err(("document parsing is turned off for this organization".into(), true));
    }
    let key = MediaKey::parse(media_key).map_err(|e| (e.to_string(), true))?;
    let media = state.org_media(org_db).map_err(|e| (format!("storage: {e}"), false))?;
    match media.head(&key).await {
        Ok(object) if object.size as usize > limits.max_input_bytes => {
            return Err((format!("files over {} MB are not parsed", limits.max_input_bytes / (1024 * 1024)), true));
        }
        Ok(_) => {}
        Err(StorageError::NotFound(_)) => return Err(("that file does not exist".into(), true)),
        Err(error) => return Err((format!("storage is unavailable ({error})"), false)),
    }
    let bytes = media.get(&key).await.map_err(|e| (format!("storage is unavailable ({e})"), false))?;
    let extension = extension_for(&bytes, name).ok_or(("this kind of file is not parsed".to_string(), true))?;
    if let Some(value) = aether_ocr::plain_text_result(&bytes, extension, limits.max_text_bytes) {
        return Ok(value);
    }
    // A job may wait for a free place much longer than a plugin call can.
    let permit = gate()
        .acquire(limits.max_concurrent, Duration::from_secs(60))
        .await
        .ok_or(("the document reader is busy".to_string(), false))?;
    let input = TempInput::create(&bytes, extension).map_err(|e| (format!("the work folder: {e}"), false))?;
    let answer = crate::bridges::call_internal(
        state,
        org_db,
        engine,
        "parse",
        &json!({
            "path": input.path(), "language": options.language, "pages": options.pages, "ocr": options.ocr,
            "max_pages": limits.max_pages, "max_text_bytes": limits.max_text_bytes,
        }),
    )
    .await;
    drop(permit);
    let value = answer.map_err(|e| (e.message, e.permanent))?;
    // Checked here too, so a result of another shape is never stored.
    serde_json::from_value::<OcrResult>(value.clone()).map_err(|e| (format!("the engine's answer: {e}"), true))?;
    Ok(value)
}

