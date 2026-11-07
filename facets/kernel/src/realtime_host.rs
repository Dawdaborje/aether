//! The `realtime::` commands a plugin can call (capabilities `realtime::publish`, `realtime::drop`,
//! `realtime::presence`). A plugin can only reach topics it declared in `[[realtime]]`, in its own namespace.

use serde_json::{Value as JsonValue, json};

use crate::kernel::host::{context::PluginHostContext, error::HostError};

const MAX_PAYLOAD: usize = 16 * 1024;

fn text<'a>(payload: &'a JsonValue, key: &str) -> Result<&'a str, HostError> {
    payload
        .get(key)
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| HostError::InvalidPayload(format!("`{key}` is required")))
}

/// The history size of the declared pattern that fits `topic` (a name without the plugin prefix).
fn declared(ctx: &PluginHostContext, topic: &str) -> Result<usize, HostError> {
    ctx.realtime_topics
        .iter()
        .find(|def| def.match_topic(topic).is_some())
        .map(|def| def.history)
        .ok_or_else(|| {
            HostError::InvalidPayload(format!(
                "`{topic}` is not a topic this plugin declared (`[[realtime]]` in plugin.toml)"
            ))
        })
}

fn full(ctx: &PluginHostContext, topic: &str) -> String {
    format!("{}:{topic}", ctx.plugin_name)
}

/// `realtime::publish { topic, event, payload, except_user? }` -> `{ seq }`
pub fn publish(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    let topic = text(payload, "topic")?;
    let event = text(payload, "event")?;
    if event.len() > 64 {
        return Err(HostError::InvalidPayload("`event` is at most 64 bytes".into()));
    }
    let data = payload.get("payload").cloned().unwrap_or(JsonValue::Null);
    let size = serde_json::to_vec(&data).map(|bytes| bytes.len()).unwrap_or(usize::MAX);
    if size > MAX_PAYLOAD {
        return Err(HostError::InvalidPayload(format!("`payload` is over {MAX_PAYLOAD} bytes")));
    }
    let history = declared(ctx, topic)?;
    let except = payload.get("except_user").and_then(JsonValue::as_str);
    let seq = ctx.notifications.realtime().publish(&ctx.database, &full(ctx, topic), history, event, data, except);
    Ok(json!({ "ok": true, "seq": seq }))
}

/// `realtime::drop { topic, user }` -> `{ dropped }`
pub fn drop_user(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    let topic = text(payload, "topic")?;
    let user = text(payload, "user")?;
    declared(ctx, topic)?;
    let dropped = ctx.notifications.realtime().drop_user(&ctx.database, &full(ctx, topic), user);
    Ok(json!({ "ok": true, "dropped": dropped }))
}

/// `realtime::presence { topic }` -> `{ users, idle }`: everyone connected, and the ones among them who are not looking at it.
pub fn presence(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    let topic = text(payload, "topic")?;
    declared(ctx, topic)?;
    let state = ctx.notifications.realtime().presence_state(&ctx.database, &full(ctx, topic));
    let users: Vec<&str> = state.iter().map(|(user, _)| user.as_str()).collect();
    let idle: Vec<&str> = state.iter().filter(|(_, idle)| *idle).map(|(user, _)| user.as_str()).collect();
    Ok(json!({ "users": users, "idle": idle }))
}
