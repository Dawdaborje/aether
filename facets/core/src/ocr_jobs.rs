//! Document parsing that runs as a job, and the results kept for it.
//!
//! `ocr::submit` writes a row to `ocr_result` and queues a job of kind `ocr`; a worker reads the file
//! from the organization's storage, runs the engine, stores the answer, and calls back the plugin
//! functions that asked (`on_done`). `ocr::parse` and `ocr::result` use the same rows, so a file parsed
//! once is parsed once. See `docs/architecture/ocr.md`.

use std::time::Duration;

use aether_ocr::{Limits, OcrResult, Options, TempInput, extension_for, gate, keys};
use aether_storage::{MediaKey, StorageError};
use serde_json::{Value, json};

use crate::scheduler::queue::{Job, NewJob};
use crate::state::{AppState, Db};

/// The job kind and queue document parsing uses.
pub const KIND: &str = "ocr";
pub const QUEUE: &str = "ocr";

/// The engines the kernel has; the first is the default.
pub const ENGINES: &[&str] = &["liteparse"];

/// The engine the limits name, when the kernel has it; otherwise the default.
pub fn engine_of(limits: &Limits) -> &'static str {
    ENGINES.iter().find(|name| **name == limits.engine).copied().unwrap_or(ENGINES[0])
}

/// A plugin function to call when a parse ends.
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

/// The stored row, as JSON, or `None`.
pub async fn find(db: &Db, cache_key: &str) -> Result<Option<Value>, surrealdb::Error> {
    let mut response = db
        .query("SELECT cache_key, media_key, engine, options, status, result, error, pages, ocr_used, callbacks, date_updated FROM ocr_result WHERE cache_key = $key LIMIT 1;")
        .bind(("key", cache_key.to_string()))
        .await?
        .check()?;
    let rows: Vec<Value> = response.take(0)?;
    Ok(rows.into_iter().next())
}

/// How many parses are waiting or running.
pub async fn in_flight(db: &Db) -> Result<u64, surrealdb::Error> {
    let mut response = db
        .query("SELECT count() AS n FROM ocr_result WHERE status IN ['pending', 'running'] GROUP ALL;")
        .await?
        .check()?;
    let rows: Vec<Value> = response.take(0)?;
    Ok(rows.first().and_then(|r| r["n"].as_u64()).unwrap_or(0))
}

/// Make (or reset a failed) row `pending`, adding the callback. `true` when a job must be queued.
pub async fn request(
    db: &Db,
    cache_key: &str,
    media_key: &str,
    engine: &str,
    options: &Options,
    callback: Option<&Callback>,
    requested_by: Option<&str>,
) -> Result<bool, surrealdb::Error> {
    let existing = find(db, cache_key).await?;
    let callbacks: Vec<Value> = callback.map(|c| vec![c.to_json()]).unwrap_or_default();
    match existing.as_ref().and_then(|r| r["status"].as_str()) {
        Some("done") | Some("pending") | Some("running") => {
            if let Some(callback) = callback {
                db.query("UPDATE ocr_result SET callbacks += $callback WHERE cache_key = $key;")
                    .bind(("callback", callback.to_json()))
                    .bind(("key", cache_key.to_string()))
                    .await?
                    .check()?;
            }
            Ok(false)
        }
        _ => {
            db.query(
                "UPSERT type::record('ocr_result', $key) CONTENT { cache_key: $key, media_key: $media, engine: $engine, \
                 options: $options, status: 'pending', result: NONE, error: NONE, pages: NONE, ocr_used: NONE, \
                 callbacks: $callbacks, requested_by: $by };",
            )
            .bind(("key", cache_key.to_string()))
            .bind(("media", media_key.to_string()))
            .bind(("engine", engine.to_string()))
            .bind(("options", serde_json::to_value(options).unwrap_or(Value::Null)))
            .bind(("callbacks", callbacks))
            .bind(("by", requested_by.map(str::to_string)))
            .await?
            .check()?;
            Ok(true)
        }
    }
}

/// Keep a finished parse (also used by a direct `ocr::parse`, so a repeat is free).
pub async fn store_done(
    db: &Db,
    cache_key: &str,
    media_key: &str,
    engine: &str,
    options: &Options,
    result: &Value,
) -> Result<(), surrealdb::Error> {
    let pages = result["pages"].as_array().map(|p| p.len() as i64);
    let ocr_used = result["ocr_used"].as_bool();
    db.query(
        "UPSERT type::record('ocr_result', $key) MERGE { cache_key: $key, media_key: $media, engine: $engine, options: $options, \
         status: 'done', result: $result, error: NONE, pages: $pages, ocr_used: $ocr_used };",
    )
    .bind(("key", cache_key.to_string()))
    .bind(("media", media_key.to_string()))
    .bind(("engine", engine.to_string()))
    .bind(("options", serde_json::to_value(options).unwrap_or(Value::Null)))
    .bind(("result", result.clone()))
    .bind(("pages", pages))
    .bind(("ocr_used", ocr_used))
    .await?
    .check()?;
    Ok(())
}

async fn set_status(db: &Db, cache_key: &str, status: &str, error: Option<&str>) -> Result<(), surrealdb::Error> {
    db.query("UPDATE ocr_result SET status = $status, error = $error WHERE cache_key = $key;")
        .bind(("status", status.to_string()))
        .bind(("error", error.map(|e| e.chars().take(500).collect::<String>())))
        .bind(("key", cache_key.to_string()))
        .await?
        .check()?;
    Ok(())
}

/// A job that could not be queued must not leave a `pending` row that nothing will ever run.
pub async fn mark_failed(db: &Db, cache_key: &str, message: &str) {
    let _ = set_status(db, cache_key, "failed", Some(message)).await;
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

/// The payload a plugin's `on_done` function receives. The text itself is fetched with `ocr::result`,
/// so a job payload stays small.
pub fn callback_payload(row: &Value, callback: &Callback) -> Value {
    json!({
        "event": "ocr.done",
        "cache_key": row["cache_key"], "media_key": row["media_key"], "status": row["status"],
        "error": row["error"], "pages": row["pages"], "ocr_used": row["ocr_used"], "ref": callback.reference,
    })
}

/// Queue the plugin functions that asked to be told. One failing to queue does not stop the others.
pub async fn fire_callbacks(db: &Db, row: &Value, state: &AppState, org_db: &str) {
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
    state.scheduler.wake(org_db);
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
                fire_callbacks(&org, &row, state, org_db).await;
            }
            Ok(())
        }
        Err((message, permanent)) => {
            if permanent || final_attempt {
                let _ = set_status(&org, &cache_key, "failed", Some(&message)).await;
                if let Ok(Some(row)) = find(&org, &cache_key).await {
                    fire_callbacks(&org, &row, state, org_db).await;
                }
            } else {
                // The queue tries again; the row says it is waiting.
                let _ = set_status(&org, &cache_key, "pending", Some(&message)).await;
            }
            Err((message, permanent))
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_engine_name_falls_back_to_the_default() {
        let mut limits = Limits::default();
        assert_eq!(engine_of(&limits), "liteparse");
        limits.engine = "nonsense".into();
        assert_eq!(engine_of(&limits), "liteparse");
    }

    #[test]
    fn callbacks_are_read_from_the_row_and_get_a_small_payload() {
        let row = json!({
            "cache_key": "k", "media_key": "media/2026/10/a.pdf", "status": "done", "error": null, "pages": 3, "ocr_used": true,
            "callbacks": [{ "plugin": "hr_recruitment", "function": "resume_parsed", "ref": "app-1" }, { "plugin": "x" }],
        });
        let callbacks = callbacks_of(&row);
        // The second names no function: it is skipped, not guessed at.
        assert_eq!(callbacks.len(), 1);
        assert_eq!(callbacks[0].plugin, "hr_recruitment");
        let payload = callback_payload(&row, &callbacks[0]);
        assert_eq!(payload["event"], "ocr.done");
        assert_eq!(payload["ref"], "app-1");
        assert_eq!(payload["status"], "done");
        // The text is fetched with `ocr::result`; it is never in the payload.
        assert!(payload.get("result").is_none() && payload.get("text").is_none());
    }
}
