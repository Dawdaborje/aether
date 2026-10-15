//! `cache::*` host commands.
//!
//! Every plugin has its own corner of the application cache, per organization: the namespace
//! is `plugin:<organization>:<plugin>`, so one plugin can neither read nor clear another's
//! entries, and one organization never sees another's. Values are JSON.
//!
//! Structured helpers (`cache::set_row`, …) build colon keys under that namespace so Mantle
//! can meter and invalidate by model. See `docs/architecture/mantle.md`.

use std::time::Duration;

use serde::Deserialize;
use serde_json::Value as JsonValue;

use crate::cache::{agg_key, list_key, meta_key, row_key};

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

#[derive(Deserialize)]
struct RowKeyRequest {
    model: String,
    id: String,
}

#[derive(Deserialize)]
struct RowSetRequest {
    model: String,
    id: String,
    value: JsonValue,
    #[serde(default)]
    ttl_secs: Option<u64>,
}

#[derive(Deserialize)]
struct ListKeyRequest {
    model: String,
    query_hash: String,
}

#[derive(Deserialize)]
struct ListSetRequest {
    model: String,
    query_hash: String,
    value: JsonValue,
    #[serde(default)]
    ttl_secs: Option<u64>,
}

#[derive(Deserialize)]
struct MetaKeyRequest {
    name: String,
}

#[derive(Deserialize)]
struct MetaSetRequest {
    name: String,
    value: JsonValue,
    #[serde(default)]
    ttl_secs: Option<u64>,
}

#[derive(Deserialize)]
struct AggSetRequest {
    model: String,
    name: String,
    scope_hash: String,
    value: JsonValue,
    #[serde(default)]
    ttl_secs: Option<u64>,
}

fn namespace(ctx: &PluginHostContext) -> String {
    format!("plugin:{}:{}", ctx.database, ctx.plugin_name)
}

fn parse<T: serde::de::DeserializeOwned>(payload: &JsonValue) -> Result<T, HostError> {
    serde_json::from_value(payload.clone())
        .map_err(|error| HostError::InvalidPayload(error.to_string()))
}

fn check_key(key: &str) -> Result<(), HostError> {
    if key.is_empty() || key.len() > MAX_KEY_BYTES || key.chars().any(char::is_control) {
        return Err(HostError::InvalidPayload(format!(
            "a cache key is 1 to {MAX_KEY_BYTES} bytes with no control characters"
        )));
    }
    Ok(())
}

fn check_segment(name: &str, label: &str) -> Result<(), HostError> {
    if name.is_empty()
        || name.len() > MAX_KEY_BYTES
        || name.contains(':')
        || name.chars().any(char::is_control)
    {
        return Err(HostError::InvalidPayload(format!(
            "cache {label} must be 1 to {MAX_KEY_BYTES} bytes with no ':' or control characters"
        )));
    }
    Ok(())
}

fn cache_error(error: crate::cache::CacheError) -> HostError {
    HostError::Message(error.to_string())
}

fn parse_ttl(ttl_secs: Option<u64>) -> Result<Option<Duration>, HostError> {
    match ttl_secs {
        Some(0) => Err(HostError::InvalidPayload(
            "ttl_secs must be at least 1".into(),
        )),
        Some(secs) => Ok(Some(Duration::from_secs(secs.min(MAX_TTL_SECS)))),
        None => Ok(None),
    }
}

fn read_json(ctx: &PluginHostContext, key: &str) -> Result<JsonValue, HostError> {
    check_key(key)?;
    let found = ctx
        .services()?
        .cache
        .get(&namespace(ctx), key)
        .map_err(cache_error)?;
    let data = match found {
        Some(value) => serde_json::from_slice(value.as_slice()).unwrap_or(JsonValue::Null),
        None => JsonValue::Null,
    };
    Ok(serde_json::json!({ "ok": true, "data": data }))
}

fn write_json(
    ctx: &PluginHostContext,
    key: &str,
    value: &JsonValue,
    ttl_secs: Option<u64>,
) -> Result<JsonValue, HostError> {
    check_key(key)?;
    let ttl = parse_ttl(ttl_secs)?;
    let bytes =
        serde_json::to_vec(value).map_err(|error| HostError::InvalidPayload(error.to_string()))?;
    ctx.services()?
        .cache
        .set(&namespace(ctx), key, bytes, ttl)
        .map_err(cache_error)?;
    Ok(serde_json::json!({ "ok": true, "data": null }))
}

pub fn cache_get(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("cache::get")?;
    let request: KeyRequest = parse(payload)?;
    read_json(ctx, &request.key)
}

pub fn cache_set(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("cache::set")?;
    let request: SetRequest = parse(payload)?;
    write_json(ctx, &request.key, &request.value, request.ttl_secs)
}

pub fn cache_get_row(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("cache::get")?;
    let request: RowKeyRequest = parse(payload)?;
    check_segment(&request.model, "model")?;
    check_segment(&request.id, "id")?;
    read_json(ctx, &row_key(&request.model, &request.id))
}

pub fn cache_set_row(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("cache::set")?;
    let request: RowSetRequest = parse(payload)?;
    check_segment(&request.model, "model")?;
    check_segment(&request.id, "id")?;
    write_json(
        ctx,
        &row_key(&request.model, &request.id),
        &request.value,
        request.ttl_secs,
    )
}

pub fn cache_invalidate_row(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("cache::invalidate")?;
    let request: RowKeyRequest = parse(payload)?;
    check_segment(&request.model, "model")?;
    check_segment(&request.id, "id")?;
    let removed = ctx
        .services()?
        .cache
        .invalidate(&namespace(ctx), &row_key(&request.model, &request.id))
        .map_err(cache_error)?;
    Ok(serde_json::json!({ "ok": true, "data": { "removed": usize::from(removed) } }))
}

pub fn cache_get_list(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("cache::get")?;
    let request: ListKeyRequest = parse(payload)?;
    check_segment(&request.model, "model")?;
    check_segment(&request.query_hash, "query_hash")?;
    read_json(ctx, &list_key(&request.model, &request.query_hash))
}

pub fn cache_set_list(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("cache::set")?;
    let request: ListSetRequest = parse(payload)?;
    check_segment(&request.model, "model")?;
    check_segment(&request.query_hash, "query_hash")?;
    write_json(
        ctx,
        &list_key(&request.model, &request.query_hash),
        &request.value,
        request.ttl_secs,
    )
}

pub fn cache_get_meta(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("cache::get")?;
    let request: MetaKeyRequest = parse(payload)?;
    check_segment(&request.name, "name")?;
    read_json(ctx, &meta_key(&request.name))
}

pub fn cache_set_meta(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("cache::set")?;
    let request: MetaSetRequest = parse(payload)?;
    check_segment(&request.name, "name")?;
    write_json(
        ctx,
        &meta_key(&request.name),
        &request.value,
        request.ttl_secs,
    )
}

pub fn cache_set_agg(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("cache::set")?;
    let request: AggSetRequest = parse(payload)?;
    check_segment(&request.model, "model")?;
    check_segment(&request.name, "name")?;
    check_segment(&request.scope_hash, "scope_hash")?;
    write_json(
        ctx,
        &agg_key(&request.model, &request.name, &request.scope_hash),
        &request.value,
        request.ttl_secs,
    )
}

pub fn cache_invalidate(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("cache::invalidate")?;
    let request: InvalidateRequest = parse(payload)?;
    let services = ctx.services()?;
    let namespace = namespace(ctx);
    let removed = match (request.key, request.prefix) {
        (Some(key), None) => {
            check_key(&key)?;
            usize::from(
                services
                    .cache
                    .invalidate(&namespace, &key)
                    .map_err(cache_error)?,
            )
        }
        (None, Some(prefix)) => {
            check_key(&prefix)?;
            services
                .cache
                .invalidate_prefix(&namespace, &prefix)
                .map_err(cache_error)?
        }
        _ => {
            return Err(HostError::InvalidPayload(
                "give either `key` or `prefix`".into(),
            ));
        }
    };
    Ok(serde_json::json!({ "ok": true, "data": { "removed": removed } }))
}

/// Clears the plugin's own entries in this organization, not the whole organization's cache.
pub fn cache_clear(ctx: &PluginHostContext, _payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("cache::clear")?;
    let removed = ctx
        .services()?
        .cache
        .clear_namespace(&namespace(ctx))
        .map_err(cache_error)?;
    Ok(serde_json::json!({ "ok": true, "data": { "removed": removed } }))
}
