//! `storage::read`, `storage::write`, `storage::delete` and `storage::list`.
//!
//! A plugin's files live under `plugins/<plugin>/` inside its organization's media storage
//! (local directory or S3, whatever `[media]` selects). The plugin gives keys such as
//! `invoices/2026/a.pdf`; the kernel adds the prefix, so a plugin can reach neither another
//! plugin's files nor the organization's own uploads, and `..` cannot climb out of it.
//!
//! Content is `text` (UTF-8) or `base64`, in both directions. Reading a file that does not exist
//! answers `data: null`.

use aether_storage::{MediaKey, StorageError};
use base64::{Engine, engine::general_purpose::STANDARD};
use bytes::Bytes;
use serde::Deserialize;
use serde_json::Value as JsonValue;

use super::context::PluginHostContext;
use super::error::HostError;

/// Largest object a plugin may write or read through a host command.
pub const MAX_OBJECT_BYTES: usize = 10 * 1024 * 1024;
/// Most entries one `storage::list` returns.
pub const MAX_LIST_ENTRIES: usize = 1000;

#[derive(Deserialize)]
struct KeyRequest {
    key: String,
    /// `"text"` (default) or `"base64"`: how `storage::read` returns the content.
    #[serde(default)]
    encoding: Option<String>,
}

#[derive(Deserialize)]
struct WriteRequest {
    key: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    base64: Option<String>,
}

#[derive(Deserialize, Default)]
struct ListRequest {
    #[serde(default)]
    prefix: Option<String>,
}

fn parse<T: serde::de::DeserializeOwned>(payload: &JsonValue) -> Result<T, HostError> {
    serde_json::from_value(payload.clone()).map_err(|error| HostError::InvalidPayload(error.to_string()))
}

/// The plugin's key placed under its own folder.
fn plugin_key(ctx: &PluginHostContext, key: &str) -> Result<MediaKey, HostError> {
    // Checked on its own first, so a key like `../x` is refused rather than normalised.
    MediaKey::parse(key).map_err(storage_error)?;
    MediaKey::parse(&format!("plugins/{}/{key}", ctx.plugin_name)).map_err(storage_error)
}

fn folder(ctx: &PluginHostContext) -> String {
    format!("plugins/{}/", ctx.plugin_name)
}

fn storage_error(error: StorageError) -> HostError {
    match error {
        StorageError::InvalidKey { .. } => HostError::InvalidPayload(error.to_string()),
        StorageError::NotFound(_) => HostError::Message("that file does not exist".into()),
        other => {
            // The backend's own message can name buckets and paths; the plugin gets a plain one.
            log::error!("plugin storage: {other}");
            HostError::Message("storage is unavailable".into())
        }
    }
}

pub async fn storage_write(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("storage::write")?;
    let request: WriteRequest = parse(payload)?;
    let bytes = match (request.text, request.base64) {
        (Some(text), None) => text.into_bytes(),
        (None, Some(encoded)) => STANDARD
            .decode(encoded.as_bytes())
            .map_err(|error| HostError::InvalidPayload(format!("base64: {error}")))?,
        _ => return Err(HostError::InvalidPayload("give either `text` or `base64`".into())),
    };
    if bytes.len() > MAX_OBJECT_BYTES {
        return Err(HostError::InvalidPayload(format!(
            "a file written by a plugin is at most {MAX_OBJECT_BYTES} bytes"
        )));
    }
    let key = plugin_key(ctx, &request.key)?;
    let size = bytes.len();
    ctx.services()?.media.put(&key, Bytes::from(bytes)).await.map_err(storage_error)?;
    Ok(serde_json::json!({ "ok": true, "data": { "key": request.key, "size": size } }))
}

pub async fn storage_read(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("storage::read")?;
    let request: KeyRequest = parse(payload)?;
    let key = plugin_key(ctx, &request.key)?;
    let media = &ctx.services()?.media;
    // Checked before downloading, so a large object is not pulled into memory to be refused.
    let head = match media.head(&key).await {
        Ok(head) => head,
        // A file that is not there is an answer (`data: null`), like a cache miss.
        Err(StorageError::NotFound(_)) => return Ok(serde_json::json!({ "ok": true, "data": null })),
        Err(error) => return Err(storage_error(error)),
    };
    if head.size > MAX_OBJECT_BYTES as u64 {
        return Err(HostError::Message(format!(
            "that file is {} bytes; a plugin may read at most {MAX_OBJECT_BYTES}",
            head.size
        )));
    }
    let bytes = media.get(&key).await.map_err(storage_error)?;
    let data = match request.encoding.as_deref().unwrap_or("text") {
        "text" => match String::from_utf8(bytes.to_vec()) {
            Ok(text) => serde_json::json!({ "text": text, "size": head.size }),
            Err(_) => {
                return Err(HostError::Message(
                    "that file is not UTF-8 text; read it with encoding \"base64\"".into(),
                ));
            }
        },
        "base64" => serde_json::json!({ "base64": STANDARD.encode(&bytes), "size": head.size }),
        other => return Err(HostError::InvalidPayload(format!("unknown encoding `{other}`"))),
    };
    Ok(serde_json::json!({ "ok": true, "data": data }))
}

pub async fn storage_delete(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("storage::delete")?;
    let request: KeyRequest = parse(payload)?;
    let key = plugin_key(ctx, &request.key)?;
    ctx.services()?.media.delete(&key).await.map_err(storage_error)?;
    Ok(serde_json::json!({ "ok": true, "data": null }))
}

pub async fn storage_list(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("storage::list")?;
    let request: ListRequest = if payload.is_null() { ListRequest::default() } else { parse(payload)? };
    let folder = folder(ctx);
    let prefix = match request.prefix.as_deref().filter(|prefix| !prefix.is_empty()) {
        Some(prefix) => plugin_key(ctx, prefix.trim_end_matches('/'))?,
        None => MediaKey::parse(folder.trim_end_matches('/')).map_err(storage_error)?,
    };
    let objects = ctx.services()?.media.list(Some(&prefix)).await.map_err(storage_error)?;
    let entries: Vec<JsonValue> = objects
        .into_iter()
        .filter_map(|object| {
            let key = object.key.strip_prefix(&folder)?.to_string();
            Some(serde_json::json!({ "key": key, "size": object.size }))
        })
        .take(MAX_LIST_ENTRIES)
        .collect();
    Ok(serde_json::json!({ "ok": true, "data": entries }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_cannot_climb_out_of_the_plugin_folder() {
        for bad in ["../other/a", "a/../../b", "/abs", "a//b", ""] {
            assert!(MediaKey::parse(bad).is_err(), "{bad}");
        }
    }
}
