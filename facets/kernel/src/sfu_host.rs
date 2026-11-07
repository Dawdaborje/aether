//! The `sfu::` commands a plugin can call (capabilities `sfu::room`, `sfu::mute`, `sfu::remove`, `sfu::end`,
//! `sfu::lock`). A plugin can only reach rooms it declared in `[[sfu]]`, in its own namespace, in its own organization.

use aether_sfu::{protocol::MediaKind, room};
use serde_json::{Value as JsonValue, json};

use crate::kernel::host::{context::PluginHostContext, error::HostError};

fn text<'a>(payload: &'a JsonValue, key: &str) -> Result<&'a str, HostError> {
    payload
        .get(key)
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| HostError::InvalidPayload(format!("`{key}` is required")))
}

/// The engine's name for `room` (a room as the plugin names it), when the plugin declared a room of that shape.
fn engine_room(ctx: &PluginHostContext, name: &str) -> Result<String, HostError> {
    ctx.sfu_rooms
        .iter()
        .find(|def| def.match_room(name).is_some())
        .map(|_| room::key(&ctx.database, &ctx.plugin_name, name))
        .ok_or_else(|| {
            HostError::InvalidPayload(format!("`{name}` is not a room this plugin declared (`[[sfu]]` in plugin.toml)"))
        })
}

pub async fn run(ctx: &PluginHostContext, command: &str, payload: &JsonValue) -> Result<JsonValue, HostError> {
    let name = text(payload, "room")?;
    let key = engine_room(ctx, name)?;
    // No engine yet means nobody has ever joined a call on this server.
    let Some(engine) = ctx.notifications.sfu().get() else {
        return Ok(match command {
            "sfu::room" => json!({ "participants": [] }),
            _ => json!({ "ok": true }),
        });
    };
    match command {
        "sfu::room" => Ok(json!({ "participants": engine.roster(&key).await })),
        "sfu::mute" => {
            let kind = match text(payload, "kind")? {
                "audio" => MediaKind::Audio,
                "video" => MediaKind::Video,
                _ => return Err(HostError::InvalidPayload("`kind` is `audio` or `video`".into())),
            };
            engine.mute(&key, text(payload, "user")?, kind).await;
            Ok(json!({ "ok": true }))
        }
        "sfu::remove" => {
            let reason = payload.get("reason").and_then(JsonValue::as_str).unwrap_or("you were removed from the meeting");
            engine.remove(&key, text(payload, "user")?, &reason.chars().take(200).collect::<String>()).await;
            Ok(json!({ "ok": true }))
        }
        "sfu::end" => {
            let reason = payload.get("reason").and_then(JsonValue::as_str).unwrap_or("the meeting ended");
            engine.end(&key, &reason.chars().take(200).collect::<String>()).await;
            Ok(json!({ "ok": true }))
        }
        "sfu::lock" => {
            let locked = payload.get("locked").and_then(JsonValue::as_bool).unwrap_or(true);
            engine.lock(&key, locked).await;
            Ok(json!({ "ok": true }))
        }
        _ => Err(HostError::InvalidPayload(format!("unknown command {command}"))),
    }
}
