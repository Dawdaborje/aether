//! `cache::get`, `cache::set`, `cache::invalidate` and `cache::clear`.
//!
//! Every plugin has its own corner of the application cache, per organization: the namespace
//! is `plugin:<organization>:<plugin>`, so one plugin can neither read nor clear another's
//! entries, and one organization never sees another's. Values are JSON.

use std::time::Duration;

use serde::Deserialize;
use serde_json::Value as JsonValue;

use super::context::PluginHostContext;
use super::error::HostError;

/// Longest a plugin may ask an entry to live (30 days).
const MAX_TTL_SECS: u64 = 30 * 24 * 3600;
const MAX_KEY_BYTES: usize = 256;

#[derive(Deserialize)]
struct KeyRequest {
    key: String,
}

#[derive(Deserialize)]
struct SetRequest {
    key: String,
    value: JsonValue,
    #[serde(default)]
    ttl_secs: Option<u64>,
}

#[derive(Deserialize)]
struct InvalidateRequest {
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    prefix: Option<String>,
}

fn namespace(ctx: &PluginHostContext) -> String {
    format!("plugin:{}:{}", ctx.database, ctx.plugin_name)
}

fn parse<T: serde::de::DeserializeOwned>(payload: &JsonValue) -> Result<T, HostError> {
    serde_json::from_value(payload.clone()).map_err(|error| HostError::InvalidPayload(error.to_string()))
}

fn check_key(key: &str) -> Result<(), HostError> {
    if key.is_empty() || key.len() > MAX_KEY_BYTES || key.chars().any(char::is_control) {
        return Err(HostError::InvalidPayload(format!(
            "a cache key is 1 to {MAX_KEY_BYTES} bytes with no control characters"
        )));
    }
    Ok(())
}

fn cache_error(error: crate::cache::CacheError) -> HostError {
    HostError::Message(error.to_string())
}

pub fn cache_get(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("cache::get")?;
    let request: KeyRequest = parse(payload)?;
    check_key(&request.key)?;
    let found = ctx
        .services()?
        .cache
        .get(&namespace(ctx), &request.key)
        .map_err(cache_error)?;
    let data = match found {
        // A value that is not JSON was not written by this command; treat it as absent.
        Some(value) => serde_json::from_slice(value.as_slice()).unwrap_or(JsonValue::Null),
        None => JsonValue::Null,
    };
    Ok(serde_json::json!({ "ok": true, "data": data }))
}

pub fn cache_set(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("cache::set")?;
    let request: SetRequest = parse(payload)?;
    check_key(&request.key)?;
    let ttl = match request.ttl_secs {
        Some(0) => return Err(HostError::InvalidPayload("ttl_secs must be at least 1".into())),
        Some(secs) => Some(Duration::from_secs(secs.min(MAX_TTL_SECS))),
        None => None,
    };
    let bytes = serde_json::to_vec(&request.value).map_err(|error| HostError::InvalidPayload(error.to_string()))?;
    ctx.services()?
        .cache
        .set(&namespace(ctx), &request.key, bytes, ttl)
        .map_err(cache_error)?;
    Ok(serde_json::json!({ "ok": true, "data": null }))
}

pub fn cache_invalidate(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("cache::invalidate")?;
    let request: InvalidateRequest = parse(payload)?;
    let services = ctx.services()?;
    let namespace = namespace(ctx);
    let removed = match (request.key, request.prefix) {
        (Some(key), None) => {
            check_key(&key)?;
            usize::from(services.cache.invalidate(&namespace, &key).map_err(cache_error)?)
        }
        (None, Some(prefix)) => {
            check_key(&prefix)?;
            services.cache.invalidate_prefix(&namespace, &prefix).map_err(cache_error)?
        }
        _ => return Err(HostError::InvalidPayload("give either `key` or `prefix`".into())),
    };
    Ok(serde_json::json!({ "ok": true, "data": { "removed": removed } }))
}

/// Clears the plugin's own entries in this organization, not the whole organization's cache.
pub fn cache_clear(ctx: &PluginHostContext, _payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("cache::clear")?;
    let removed = ctx.services()?.cache.clear_namespace(&namespace(ctx)).map_err(cache_error)?;
    Ok(serde_json::json!({ "ok": true, "data": { "removed": removed } }))
}
