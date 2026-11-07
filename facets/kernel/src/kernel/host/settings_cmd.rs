//! `settings::get` and `settings::set`: a plugin reads the settings of the organization it is serving
//! (its own value if the organization has one, else the global one) and changes its own.
//!
//! Secrets are never given to a plugin. A plugin changes only keys that start with its own name and a
//! dot, which are the ones its manifest declares with `[[setting]]`, and only for an administrator of
//! the organization (or the kernel itself, as an install seed runs).

use std::collections::HashSet;

use serde_json::{Map, Value as JsonValue, json};

use super::context::PluginHostContext;
use super::error::HostError;

const MAX_KEYS: usize = 50;

fn keys_of(payload: &JsonValue) -> Result<Vec<String>, HostError> {
    let mut keys = Vec::new();
    if let Some(key) = payload.get("key").and_then(JsonValue::as_str) {
        keys.push(key.trim().to_string());
    }
    if let Some(list) = payload.get("keys").and_then(JsonValue::as_array) {
        for key in list {
            let key = key
                .as_str()
                .ok_or_else(|| HostError::InvalidPayload("`keys` is a list of text".into()))?;
            keys.push(key.trim().to_string());
        }
    }
    keys.retain(|key| !key.is_empty());
    if keys.is_empty() {
        return Err(HostError::InvalidPayload("settings::get needs `key` or `keys`".into()));
    }
    if keys.len() > MAX_KEYS {
        return Err(HostError::InvalidPayload(format!("ask for at most {MAX_KEYS} settings at a time")));
    }
    Ok(keys)
}

async fn rows(db: &crate::databases::Db, table: &str, keys: &[String]) -> Result<Vec<JsonValue>, HostError> {
    let query = format!("SELECT s_key, s_value, is_secret FROM {table} WHERE s_key IN $keys;");
    let mut response = db
        .query(&query)
        .bind(("keys", keys.to_vec()))
        .await
        .map_err(HostError::Db)?
        .check()
        .map_err(HostError::Db)?;
    response.take(0).map_err(HostError::Db)
}

/// `{ key: value }` for each asked key that has a value; a key nobody set is left out.
pub async fn get(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("settings::get")?;
    let keys = keys_of(payload)?;
    let mut secret: HashSet<String> = HashSet::new();
    let mut found: Map<String, JsonValue> = Map::new();
    let core = ctx.services().ok().and_then(|services| services.core.clone());
    let mut tables: Vec<(&crate::databases::Db, &str)> = Vec::new();
    if let Some(core) = core.as_ref() {
        tables.push((core, "gl_settings_items"));
    }
    tables.push((&ctx.db, "settings_items"));
    // Global first, then the organization's own on top.
    for (db, table) in tables {
        for row in rows(db, table, &keys).await? {
            let Some(key) = row.get("s_key").and_then(JsonValue::as_str) else { continue };
            if row.get("is_secret").and_then(JsonValue::as_bool) == Some(true) {
                secret.insert(key.to_string());
                continue;
            }
            if let Some(value) = row.get("s_value") {
                found.insert(key.to_string(), value.clone());
            }
        }
    }
    for key in &secret {
        found.remove(key);
    }
    Ok(json!({ "ok": true, "data": JsonValue::Object(found) }))
}

async fn caller_may_change(ctx: &PluginHostContext) -> Result<bool, HostError> {
    match (ctx.audit.actor.kind(), ctx.audit.actor.id()) {
        ("system", _) => Ok(true),
        ("user", Some(user)) => {
            let roles = crate::roles::roles_of(&ctx.db, user).await.map_err(HostError::Db)?;
            Ok(roles.iter().any(|role| role == "org_admin"))
        }
        _ => Ok(false),
    }
}

/// Set this organization's value of one of the plugin's own settings.
pub async fn set(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("settings::set")?;
    let key = payload
        .get("key")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or_else(|| HostError::InvalidPayload("settings::set needs `key`".into()))?;
    let prefix = format!("{}.", ctx.plugin_name);
    if !key.starts_with(&prefix) {
        return Err(HostError::Denied(format!(
            "a plugin changes only its own settings, whose keys start with `{prefix}`"
        )));
    }
    let value = payload.get("value").cloned().unwrap_or(JsonValue::Null);
    let value = match value {
        JsonValue::Null => JsonValue::String(String::new()),
        JsonValue::String(_) | JsonValue::Number(_) | JsonValue::Bool(_) => value,
        _ => return Err(HostError::InvalidPayload("a setting's value is text, a number or true/false".into())),
    };
    if !caller_may_change(ctx).await? {
        return Err(HostError::Denied("only an administrator of the organization can change settings".into()));
    }
    let core = ctx
        .services()?
        .core
        .as_ref()
        .ok_or_else(|| HostError::Message("settings need the core database".into()))?;
    let mut meta = core
        .query("SELECT label, description, long_description, is_secret FROM gl_settings_items WHERE s_key = $key LIMIT 1;")
        .bind(("key", key.to_string()))
        .await
        .map_err(HostError::Db)?
        .check()
        .map_err(HostError::Db)?;
    let meta: Vec<JsonValue> = meta.take(0).map_err(HostError::Db)?;
    let Some(meta) = meta.into_iter().next() else {
        return Err(HostError::InvalidPayload(format!(
            "there is no setting `{key}`: declare it in plugin.toml with [[setting]]"
        )));
    };
    if meta.get("is_secret").and_then(JsonValue::as_bool) == Some(true) {
        return Err(HostError::Denied("a secret setting cannot be changed by a plugin".into()));
    }
    let existing = rows(&ctx.db, "settings_items", &[key.to_string()]).await?;
    if existing.is_empty() {
        ctx.db
            .query(
                "CREATE settings_items SET label = $label, s_key = $key, s_value = $value, is_secret = false, \
                 description = $description, long_description = $long_description;",
            )
            .bind(("key", key.to_string()))
            .bind(("label", meta.get("label").and_then(JsonValue::as_str).unwrap_or(key).to_string()))
            .bind(("value", value.clone()))
            .bind(("description", meta.get("description").and_then(JsonValue::as_str).map(str::to_string)))
            .bind(("long_description", meta.get("long_description").and_then(JsonValue::as_str).map(str::to_string)))
            .await
            .map_err(HostError::Db)?
            .check()
            .map_err(HostError::Db)?;
    } else {
        ctx.db
            .query("UPDATE settings_items SET s_value = $value WHERE s_key = $key;")
            .bind(("key", key.to_string()))
            .bind(("value", value.clone()))
            .await
            .map_err(HostError::Db)?
            .check()
            .map_err(HostError::Db)?;
    }
    // The settings screen and other readers keep a short-lived copy; drop it.
    if let Ok(services) = ctx.services() {
        let _ = services.cache.warm_invalidate_setting(Some(ctx.database.as_str()), key);
    }
    Ok(json!({ "ok": true, "data": { "key": key, "value": value } }))
}
