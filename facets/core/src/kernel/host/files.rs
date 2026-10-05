//! `fs::read`, `fs::write`, `fs::list`, `fs::stat`, `fs::rename` and `fs::delete`: a plugin's folder on
//! disk, `<app_dir>/orgs/<organization>/plugins/<plugin>/`.
//!
//! It is the folder a `[[watch]]` listens to: an administrator or another program drops files there,
//! the watch tells the plugin, and the plugin reads, processes and moves them (typically into a
//! `done/` folder, so each file is handled once). Paths are relative to the folder; `..`, absolute
//! paths and symbolic links that lead out of it are refused (see [`crate::plugin_files`]). A file is at
//! most 10 MiB through these commands. For files that belong to the organization and may live in S3,
//! use `storage::*` instead; this folder is always on local disk.
//!
//! Writes go to a scratch name and are renamed into place, so a watcher never sees a half-written
//! file.

use std::path::PathBuf;

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::Value as JsonValue;

use crate::plugin_files::{PathError, relative_to, resolve};

use super::context::PluginHostContext;
use super::error::HostError;

/// Largest file a plugin may read or write through these commands.
pub const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
/// Most entries one `fs::list` returns.
pub const MAX_LIST_ENTRIES: usize = 1000;

#[derive(Deserialize)]
struct PathRequest {
    path: String,
    /// `"text"` (default) or `"base64"`: how `fs::read` returns the content.
    #[serde(default)]
    encoding: Option<String>,
}

#[derive(Deserialize)]
struct WriteRequest {
    path: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    base64: Option<String>,
}

#[derive(Deserialize)]
struct RenameRequest {
    from: String,
    to: String,
    /// Replace the destination if it exists (default: refuse).
    #[serde(default)]
    overwrite: bool,
}

#[derive(Deserialize, Default)]
struct ListRequest {
    #[serde(default)]
    path: String,
}

fn parse<T: serde::de::DeserializeOwned>(payload: &JsonValue) -> Result<T, HostError> {
    serde_json::from_value(payload.clone()).map_err(|error| HostError::InvalidPayload(error.to_string()))
}

fn path_error(error: PathError) -> HostError {
    match error {
        PathError::Unsafe(_) | PathError::Escapes(_) => HostError::InvalidPayload(error.to_string()),
        PathError::Folder(reason) => {
            log::error!("plugin folder: {reason}");
            HostError::Message("the plugin's folder is unavailable".into())
        }
    }
}

fn io_error(error: std::io::Error) -> HostError {
    match error.kind() {
        std::io::ErrorKind::NotFound => HostError::Message("that file or folder does not exist".into()),
        std::io::ErrorKind::PermissionDenied => HostError::Message("the kernel may not do that in the plugin's folder".into()),
        _ => {
            // The operating system's message can name host paths; the plugin gets a plain one.
            log::error!("plugin folder: {error}");
            HostError::Message("the plugin's folder could not be used".into())
        }
    }
}

/// The plugin's folder, created if it is not there yet.
async fn root(ctx: &PluginHostContext) -> Result<PathBuf, HostError> {
    let root = ctx
        .services()?
        .files_root
        .clone()
        .ok_or_else(|| HostError::Message("this call has no folder on disk".into()))?;
    tokio::fs::create_dir_all(&root).await.map_err(io_error)?;
    Ok(root)
}

fn modified_ns(metadata: &std::fs::Metadata) -> Option<String> {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_nanos().to_string())
}

pub async fn read(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("fs::read")?;
    let request: PathRequest = parse(payload)?;
    let root = root(ctx).await?;
    let full = resolve(&root, &request.path).await.map_err(path_error)?;
    let metadata = match tokio::fs::metadata(&full).await {
        Ok(metadata) => metadata,
        // A file that is not there is an answer, like a cache miss.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(serde_json::json!({ "ok": true, "data": null })),
        Err(error) => return Err(io_error(error)),
    };
    if !metadata.is_file() {
        return Err(HostError::Message("that is a folder, not a file".into()));
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(HostError::Message(format!("that file is {} bytes; a plugin may read at most {MAX_FILE_BYTES}", metadata.len())));
    }
    let bytes = tokio::fs::read(&full).await.map_err(io_error)?;
    let data = match request.encoding.as_deref().unwrap_or("text") {
        "text" => match String::from_utf8(bytes) {
            Ok(text) => serde_json::json!({ "text": text, "size": metadata.len() }),
            Err(_) => return Err(HostError::Message("that file is not UTF-8 text; read it with encoding \"base64\"".into())),
        },
        "base64" => serde_json::json!({ "base64": STANDARD.encode(&bytes), "size": metadata.len() }),
        other => return Err(HostError::InvalidPayload(format!("unknown encoding `{other}`"))),
    };
    Ok(serde_json::json!({ "ok": true, "data": data }))
}

pub async fn write(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("fs::write")?;
    let request: WriteRequest = parse(payload)?;
    let bytes = match (request.text, request.base64) {
        (Some(text), None) => text.into_bytes(),
        (None, Some(encoded)) => STANDARD
            .decode(encoded.as_bytes())
            .map_err(|error| HostError::InvalidPayload(format!("base64: {error}")))?,
        _ => return Err(HostError::InvalidPayload("give either `text` or `base64`".into())),
    };
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(HostError::InvalidPayload(format!("a file written by a plugin is at most {MAX_FILE_BYTES} bytes")));
    }
    let root = root(ctx).await?;
    if request.path.is_empty() {
        return Err(HostError::InvalidPayload("`path` names a file".into()));
    }
    let full = resolve(&root, &request.path).await.map_err(path_error)?;
    if tokio::fs::metadata(&full).await.is_ok_and(|metadata| metadata.is_dir()) {
        return Err(HostError::Message("that is a folder".into()));
    }
    let (Some(parent), Some(name)) = (full.parent(), full.file_name().and_then(|name| name.to_str())) else {
        return Err(HostError::InvalidPayload("`path` names a file".into()));
    };
    tokio::fs::create_dir_all(parent).await.map_err(io_error)?;
    // A leading dot makes watchers ignore it until it is complete.
    let scratch = parent.join(format!(".{name}.aether-write"));
    tokio::fs::write(&scratch, &bytes).await.map_err(io_error)?;
    if let Err(error) = tokio::fs::rename(&scratch, &full).await {
        let _ = tokio::fs::remove_file(&scratch).await;
        return Err(io_error(error));
    }
    Ok(serde_json::json!({ "ok": true, "data": { "path": request.path, "size": bytes.len() } }))
}

pub async fn list(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("fs::list")?;
    let request: ListRequest = if payload.is_null() { ListRequest::default() } else { parse(payload)? };
    let root = root(ctx).await?;
    let real_root = tokio::fs::canonicalize(&root).await.map_err(io_error)?;
    let dir = resolve(&root, &request.path).await.map_err(path_error)?;
    let mut entries = match tokio::fs::read_dir(&dir).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(serde_json::json!({ "ok": true, "data": [] })),
        Err(error) => return Err(io_error(error)),
    };
    let mut out = Vec::new();
    while let Some(entry) = entries.next_entry().await.map_err(io_error)? {
        // A link out of the folder is not listed.
        let Ok(real) = tokio::fs::canonicalize(entry.path()).await else { continue };
        if relative_to(&real_root, &real).is_none() {
            continue;
        }
        let Ok(metadata) = tokio::fs::metadata(&real).await else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        out.push(serde_json::json!({
            "name": name,
            "path": format!("{}{}{name}", request.path.trim_matches('/'), if request.path.trim_matches('/').is_empty() { "" } else { "/" }),
            "is_dir": metadata.is_dir(),
            "size": metadata.len(),
            "modified_ns": modified_ns(&metadata),
        }));
        if out.len() >= MAX_LIST_ENTRIES {
            break;
        }
    }
    out.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(serde_json::json!({ "ok": true, "data": out }))
}

pub async fn stat(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("fs::read")?;
    let request: PathRequest = parse(payload)?;
    let root = root(ctx).await?;
    let full = resolve(&root, &request.path).await.map_err(path_error)?;
    match tokio::fs::metadata(&full).await {
        Ok(metadata) => Ok(serde_json::json!({
            "ok": true,
            "data": { "is_dir": metadata.is_dir(), "size": metadata.len(), "modified_ns": modified_ns(&metadata) }
        })),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(serde_json::json!({ "ok": true, "data": null })),
        Err(error) => Err(io_error(error)),
    }
}

pub async fn rename(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("fs::write")?;
    let request: RenameRequest = parse(payload)?;
    if request.from.is_empty() || request.to.is_empty() {
        return Err(HostError::InvalidPayload("`from` and `to` name files".into()));
    }
    let root = root(ctx).await?;
    let from = resolve(&root, &request.from).await.map_err(path_error)?;
    let to = resolve(&root, &request.to).await.map_err(path_error)?;
    if tokio::fs::symlink_metadata(&from).await.is_err() {
        return Err(HostError::Message("the file to move does not exist".into()));
    }
    if !request.overwrite && tokio::fs::symlink_metadata(&to).await.is_ok() {
        return Err(HostError::Message("something is already there; pass `overwrite: true` to replace it".into()));
    }
    if let Some(parent) = to.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(io_error)?;
    }
    tokio::fs::rename(&from, &to).await.map_err(io_error)?;
    Ok(serde_json::json!({ "ok": true, "data": { "path": request.to } }))
}

pub async fn delete(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("fs::delete")?;
    let request: PathRequest = parse(payload)?;
    if request.path.is_empty() {
        return Err(HostError::InvalidPayload("`path` names a file or an empty folder, not the whole folder".into()));
    }
    let root = root(ctx).await?;
    let full = resolve(&root, &request.path).await.map_err(path_error)?;
    let removed = match tokio::fs::symlink_metadata(&full).await {
        Ok(metadata) if metadata.is_dir() => tokio::fs::remove_dir(&full).await,
        Ok(_) => tokio::fs::remove_file(&full).await,
        // Deleting what is not there is not an error.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(serde_json::json!({ "ok": true, "data": null })),
        Err(error) => return Err(io_error(error)),
    };
    removed.map_err(|error| match error.kind() {
        std::io::ErrorKind::DirectoryNotEmpty => HostError::Message("that folder is not empty".into()),
        _ => io_error(error),
    })?;
    Ok(serde_json::json!({ "ok": true, "data": null }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::host::{
        dispatch::kernel_command,
        test_support::{dummy_ctx, in_memory_media, with_services},
    };
    use serde_json::json;

    const CAPS: [&str; 4] = ["fs::read", "fs::write", "fs::list", "fs::delete"];

    fn context(dir: &tempfile::TempDir, caps: &[&str]) -> PluginHostContext {
        let mut ctx = with_services(dummy_ctx(caps), in_memory_media());
        if let Some(services) = ctx.services.as_mut() {
            services.files_root = Some(dir.path().join("plugin"));
        }
        ctx
    }

    #[tokio::test]
    async fn a_plugin_reads_writes_lists_moves_and_deletes_its_files() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let ctx = context(&dir, &CAPS);
        kernel_command(&ctx, "fs::write", json!({ "path": "inbox/a.csv", "text": "id,total\n1,500\n" })).await?;
        kernel_command(&ctx, "fs::write", json!({ "path": "inbox/b.bin", "base64": "//4=" })).await?;

        let read = kernel_command(&ctx, "fs::read", json!({ "path": "inbox/a.csv" })).await?;
        assert_eq!(read["data"]["text"], "id,total\n1,500\n");
        let binary = kernel_command(&ctx, "fs::read", json!({ "path": "inbox/b.bin", "encoding": "base64" })).await?;
        assert_eq!(binary["data"]["base64"], "//4=");
        assert!(kernel_command(&ctx, "fs::read", json!({ "path": "inbox/b.bin" })).await.is_err(), "not text");
        assert!(kernel_command(&ctx, "fs::read", json!({ "path": "inbox/missing" })).await?["data"].is_null());

        let listed = kernel_command(&ctx, "fs::list", json!({ "path": "inbox" })).await?;
        let names: Vec<_> = listed["data"].as_array().ok_or("no list")?.iter().filter_map(|e| e["path"].as_str()).collect();
        assert_eq!(names, ["inbox/a.csv", "inbox/b.bin"]);
        assert!(kernel_command(&ctx, "fs::list", json!({ "path": "nothing" })).await?["data"].as_array().is_some_and(Vec::is_empty));
        let stat = kernel_command(&ctx, "fs::stat", json!({ "path": "inbox/a.csv" })).await?;
        assert_eq!((stat["data"]["size"].as_u64(), stat["data"]["is_dir"].as_bool()), (Some(15), Some(false)));

        // Moving into another folder creates it; an existing target is protected unless asked.
        kernel_command(&ctx, "fs::rename", json!({ "from": "inbox/a.csv", "to": "done/2026/a.csv" })).await?;
        assert!(kernel_command(&ctx, "fs::stat", json!({ "path": "inbox/a.csv" })).await?["data"].is_null());
        kernel_command(&ctx, "fs::write", json!({ "path": "inbox/a.csv", "text": "new" })).await?;
        let blocked = kernel_command(&ctx, "fs::rename", json!({ "from": "inbox/a.csv", "to": "done/2026/a.csv" })).await;
        assert!(matches!(&blocked, Err(HostError::Message(m)) if m.contains("overwrite")), "{blocked:?}");
        kernel_command(&ctx, "fs::rename", json!({ "from": "inbox/a.csv", "to": "done/2026/a.csv", "overwrite": true })).await?;
        assert_eq!(kernel_command(&ctx, "fs::read", json!({ "path": "done/2026/a.csv" })).await?["data"]["text"], "new");

        // Delete: a file, an empty folder; not a folder with things in it; nothing is not an error.
        kernel_command(&ctx, "fs::delete", json!({ "path": "inbox/b.bin" })).await?;
        kernel_command(&ctx, "fs::delete", json!({ "path": "inbox" })).await?;
        let full = kernel_command(&ctx, "fs::delete", json!({ "path": "done" })).await;
        assert!(matches!(&full, Err(HostError::Message(m)) if m.contains("not empty")), "{full:?}");
        kernel_command(&ctx, "fs::delete", json!({ "path": "nothing/here" })).await?;
        assert!(kernel_command(&ctx, "fs::delete", json!({ "path": "" })).await.is_err(), "not the whole folder");
        Ok(())
    }

    #[tokio::test]
    async fn nothing_outside_the_plugins_folder_can_be_reached() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let ctx = context(&dir, &CAPS);
        std::fs::create_dir_all(dir.path().join("other_plugin"))?;
        std::fs::write(dir.path().join("other_plugin/secret.txt"), "classified")?;
        std::fs::write(dir.path().join("host_file"), "host")?;
        kernel_command(&ctx, "fs::write", json!({ "path": "ok.txt", "text": "fine" })).await?;

        for path in ["../other_plugin/secret.txt", "../host_file", "/etc/passwd", "a/../../host_file", "a\\b", "x//y"] {
            for command in ["fs::read", "fs::write", "fs::stat", "fs::delete"] {
                let mut request = json!({ "path": path });
                request["text"] = json!("pwned");
                let result = kernel_command(&ctx, command, request).await;
                assert!(matches!(result, Err(HostError::InvalidPayload(_))), "{command} {path}: {result:?}");
            }
            let moved = kernel_command(&ctx, "fs::rename", json!({ "from": "ok.txt", "to": path })).await;
            assert!(moved.is_err(), "rename to {path}");
        }
        assert_eq!(std::fs::read_to_string(dir.path().join("host_file"))?, "host");
        assert_eq!(std::fs::read_to_string(dir.path().join("other_plugin/secret.txt"))?, "classified");

        // A link planted in the folder that points out of it does not work, for reading, writing or listing.
        #[cfg(unix)]
        {
            let plugin = dir.path().join("plugin");
            std::os::unix::fs::symlink(dir.path().join("other_plugin"), plugin.join("link"))?;
            std::os::unix::fs::symlink(dir.path().join("host_file"), plugin.join("filelink"))?;
            for request in [("fs::read", json!({ "path": "link/secret.txt" })), ("fs::read", json!({ "path": "filelink" })), ("fs::write", json!({ "path": "link/new.txt", "text": "x" })), ("fs::delete", json!({ "path": "link/secret.txt" }))] {
                let result = kernel_command(&ctx, request.0, request.1.clone()).await;
                assert!(matches!(result, Err(HostError::InvalidPayload(_))), "{request:?}: {result:?}");
            }
            let listed = kernel_command(&ctx, "fs::list", json!({})).await?;
            let names: Vec<_> = listed["data"].as_array().ok_or("no list")?.iter().filter_map(|e| e["name"].as_str()).collect();
            assert_eq!(names, ["ok.txt"], "links that leave the folder are not even listed");
            assert!(!dir.path().join("other_plugin/new.txt").exists());
        }
        Ok(())
    }

    #[tokio::test]
    async fn sizes_capabilities_and_a_missing_folder_are_handled() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let ctx = context(&dir, &CAPS);
        let big = "x".repeat(usize::try_from(MAX_FILE_BYTES)? + 1);
        let too_big = kernel_command(&ctx, "fs::write", json!({ "path": "big.txt", "text": big })).await;
        assert!(matches!(too_big, Err(HostError::InvalidPayload(_))));
        assert!(kernel_command(&ctx, "fs::write", json!({ "path": "x", "text": "a", "base64": "AA==" })).await.is_err());
        // A scratch file is not left behind by a write.
        kernel_command(&ctx, "fs::write", json!({ "path": "clean.txt", "text": "ok" })).await?;
        let names: Vec<_> = std::fs::read_dir(dir.path().join("plugin"))?.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["clean.txt"]);
        // Each command needs its own capability.
        let reader = context(&dir, &["fs::read"]);
        for (command, payload) in [("fs::write", json!({ "path": "a", "text": "b" })), ("fs::delete", json!({ "path": "a" })), ("fs::list", json!({})), ("fs::rename", json!({ "from": "a", "to": "b" }))] {
            assert!(matches!(kernel_command(&reader, command, payload).await, Err(HostError::Capability(_))), "{command}");
        }
        assert!(kernel_command(&reader, "fs::read", json!({ "path": "clean.txt" })).await.is_ok());
        // A call with no folder says so.
        let bare = with_services(dummy_ctx(&CAPS), in_memory_media());
        assert!(matches!(kernel_command(&bare, "fs::list", json!({})).await, Err(HostError::Message(_))));
        Ok(())
    }
}
