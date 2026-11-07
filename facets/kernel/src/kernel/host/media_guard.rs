//! `media::protect` and `media::release`: a plugin that keeps files with its own permissions (a drive) puts the
//! keys of its files under its guard. Reading a guarded key then asks the plugin (`[media_guard]` in `plugin.toml`);
//! see [`crate::media_api`] and `docs/architecture/media.md`.

use serde_json::{Value as JsonValue, json};
use surrealdb::types::SurrealValue;

use super::context::PluginHostContext;
use super::error::HostError;

#[derive(Debug, SurrealValue)]
struct GuardRow {
    plugin: String,
    pending_for: Option<String>,
}

fn key_of(payload: &JsonValue) -> Result<&str, HostError> {
    let key = payload.get("key").and_then(JsonValue::as_str).map(str::trim).unwrap_or_default();
    if !crate::data_model::media::is_media_key(key) || aether_storage::MediaKey::parse(key).is_err() {
        return Err(HostError::InvalidPayload("`key` is the key of a stored file (`media/…`)".into()));
    }
    Ok(key)
}

fn require_guard(ctx: &PluginHostContext) -> Result<(), HostError> {
    if ctx.has_media_guard {
        Ok(())
    } else {
        Err(HostError::InvalidPayload(
            "declare `[media_guard]` with the function that decides who may read, then guard files".into(),
        ))
    }
}

/// Who guards `key` now, if anyone.
pub async fn guard_of(db: &crate::databases::Db, key: &str) -> Result<Option<String>, surrealdb::Error> {
    Ok(guard_row(db, key).await?.map(|(plugin, _)| plugin))
}

/// Who guards `key` and, while the plugin has not yet registered it, the one person who uploaded it.
pub async fn guard_row(db: &crate::databases::Db, key: &str) -> Result<Option<(String, Option<String>)>, surrealdb::Error> {
    let mut response = db
        .query("SELECT plugin, pending_for FROM media_guards WHERE key = $key LIMIT 1;")
        .bind(("key", key.to_string()))
        .await?
        .check()?;
    let rows: Vec<GuardRow> = response.take(0)?;
    Ok(rows.into_iter().next().map(|row| (row.plugin, row.pending_for)))
}

/// Guard a key from the moment of its upload, readable only by `uploader` until the plugin registers it.
pub async fn guard_pending(db: &crate::databases::Db, key: &str, plugin: &str, uploader: &str) -> Result<(), surrealdb::Error> {
    db.query("CREATE media_guards SET key = $key, plugin = $plugin, pending_for = $uploader;")
        .bind(("key", key.to_string()))
        .bind(("plugin", plugin.to_string()))
        .bind(("uploader", uploader.to_string()))
        .await?
        .check()?;
    Ok(())
}

pub async fn protect(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("media::protect")?;
    require_guard(ctx)?;
    let key = key_of(payload)?;
    match guard_of(&ctx.db, key).await.map_err(HostError::Db)? {
        Some(plugin) if plugin == ctx.plugin_name => {
            if !ctx.dry_run {
                ctx.db
                    .query("UPDATE media_guards SET pending_for = NONE WHERE key = $key;")
                    .bind(("key", key.to_string()))
                    .await
                    .map_err(HostError::Db)?
                    .check()
                    .map_err(HostError::Db)?;
            }
            return Ok(json!({ "ok": true, "already": true }));
        }
        Some(_) => return Err(HostError::InvalidPayload("that file is guarded by another plugin".into())),
        None => {}
    }
    if ctx.dry_run {
        return Ok(json!({ "ok": true }));
    }
    ctx.db
        .query("CREATE media_guards SET key = $key, plugin = $plugin;")
        .bind(("key", key.to_string()))
        .bind(("plugin", ctx.plugin_name.clone()))
        .await
        .map_err(HostError::Db)?
        .check()
        .map_err(HostError::Db)?;
    Ok(json!({ "ok": true }))
}

pub async fn release(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("media::release")?;
    let key = key_of(payload)?;
    if ctx.dry_run {
        return Ok(json!({ "ok": true }));
    }
    ctx.db
        .query("DELETE media_guards WHERE key = $key AND plugin = $plugin;")
        .bind(("key", key.to_string()))
        .bind(("plugin", ctx.plugin_name.clone()))
        .await
        .map_err(HostError::Db)?
        .check()
        .map_err(HostError::Db)?;
    Ok(json!({ "ok": true }))
}

/// Largest file a plugin may store or read through these commands (the upload limit of the web app is the same).
const MAX_BYTES: usize = crate::data_model::media::MAX_MEDIA_BYTES;

/// `media::store { name, base64 }`: a plugin puts a file it holds (an attachment fetched from a mail server) into the
/// organization's media. The kernel decides what the file is from its bytes and refuses what an upload would refuse; a picture
/// is shrunk and stripped of its metadata as for an upload. The key is guarded for this plugin from the first moment and is
/// readable only by the person the call runs for until the plugin calls `media::protect` on it, so a plugin that wants a
/// file to be served only to that person simply never protects it.
pub async fn store(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use rand::RngExt;
    use sha2::{Digest, Sha256};

    ctx.require_cap("media::store")?;
    require_guard(ctx)?;
    let name = payload.get("name").and_then(JsonValue::as_str).unwrap_or("file");
    let encoded = payload
        .get("base64")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| HostError::InvalidPayload("`base64` is the content of the file".into()))?;
    let bytes = STANDARD.decode(encoded).map_err(|error| HostError::InvalidPayload(format!("`base64` is not valid: {error}")))?;
    if bytes.is_empty() {
        return Err(HostError::InvalidPayload("the file is empty".into()));
    }
    if bytes.len() > MAX_BYTES {
        return Err(HostError::InvalidPayload(format!("a file may be at most {} MB", MAX_BYTES / (1024 * 1024))));
    }
    let Some(kind) = crate::data_model::media::sniff(&bytes) else {
        return Err(HostError::InvalidPayload(
            "that kind of file is not allowed (pictures, PDF, Office documents, zip and plain text are)".into(),
        ));
    };
    let user = match (ctx.audit.actor.kind(), ctx.audit.actor.id()) {
        ("user", Some(user)) => user.to_string(),
        _ => return Err(HostError::InvalidPayload("a file is stored for a person: this call runs for none".into())),
    };
    let (bytes, kind, thumbnail) = if kind.is_image() {
        let source = bytes.clone();
        let done = tokio::task::spawn_blocking(move || crate::media_image::process(&source, kind))
            .await
            .map_err(|error| HostError::Message(format!("could not read the picture: {error}")))?
            .map_err(HostError::InvalidPayload)?;
        (done.bytes, done.kind, Some(done.thumbnail))
    } else {
        (bytes, kind, None)
    };
    let mut random = [0u8; 16];
    rand::rng().fill(&mut random);
    let now = chrono::Utc::now();
    let key = crate::data_model::media::new_key(chrono::Datelike::year(&now), chrono::Datelike::month(&now), random, kind);
    let size = bytes.len();
    let sha256: String = Sha256::digest(&bytes).iter().map(|byte| format!("{byte:02x}")).collect();
    if ctx.dry_run {
        return Ok(json!({ "ok": true, "data": { "key": key, "name": crate::data_model::media::clean_name(name), "mime": kind.mime(), "size": size, "sha256": sha256 } }));
    }
    let media_key = aether_storage::MediaKey::parse(&key).map_err(|error| HostError::Message(error.to_string()))?;
    ctx.services()?.media.put(&media_key, bytes::Bytes::from(bytes)).await.map_err(|error| HostError::Message(format!("storage is unavailable: {error}")))?;
    if let Some(thumbnail) = thumbnail {
        if let Ok(thumb_key) = aether_storage::MediaKey::parse(&format!("{key}{}", crate::media_image::THUMB_SUFFIX)) {
            let _ = ctx.services()?.media.put(&thumb_key, bytes::Bytes::from(thumbnail)).await;
        }
    }
    if let Err(error) = guard_pending(&ctx.db, &key, &ctx.plugin_name, &user).await {
        let _ = ctx.services()?.media.delete(&media_key).await;
        return Err(HostError::Db(error));
    }
    Ok(json!({ "ok": true, "data": { "key": key, "name": crate::data_model::media::clean_name(name), "mime": kind.mime(), "size": size, "sha256": sha256 } }))
}

/// `media::read { key }`: the bytes (`{ base64, size }`) of a file this plugin guards: one it stored, or one a person uploaded
/// for it (`POST /api/media?guard=<plugin>`) who is the person this call runs for. A plugin cannot read files it does not
/// guard, and not a file still waiting for someone else.
pub async fn read(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    use base64::{Engine, engine::general_purpose::STANDARD};

    ctx.require_cap("media::read")?;
    let key = key_of(payload)?;
    let Some((plugin, pending_for)) = guard_row(&ctx.db, key).await.map_err(HostError::Db)? else {
        return Err(HostError::InvalidPayload("that file is not guarded by this plugin".into()));
    };
    if plugin != ctx.plugin_name {
        return Err(HostError::InvalidPayload("that file is not guarded by this plugin".into()));
    }
    if let Some(waiting_for) = pending_for {
        if ctx.audit.actor.id() != Some(waiting_for.as_str()) {
            return Err(HostError::InvalidPayload("that file is waiting for someone else".into()));
        }
    }
    let media_key = aether_storage::MediaKey::parse(key).map_err(|error| HostError::Message(error.to_string()))?;
    let bytes = ctx.services()?.media.get(&media_key).await.map_err(|error| match error {
        aether_storage::StorageError::NotFound(_) => HostError::InvalidPayload("there is no such file".into()),
        other => HostError::Message(format!("storage is unavailable: {other}")),
    })?;
    if bytes.len() > MAX_BYTES {
        return Err(HostError::InvalidPayload("that file is too large to read".into()));
    }
    Ok(json!({ "ok": true, "data": { "base64": STANDARD.encode(&bytes), "size": bytes.len() } }))
}
