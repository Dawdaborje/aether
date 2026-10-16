//! Reading kernel settings from a host command: the organization's own value when it has one,
//! otherwise the global one. Used by the commands whose limits are settings (`ocr.*`).

use std::collections::HashMap;

use serde_json::Value as JsonValue;

use super::context::PluginHostContext;

async fn read_into(db: &crate::state::Db, table: &str, keys: &[String], out: &mut HashMap<String, JsonValue>) {
    let query = format!("SELECT s_key, s_value FROM {table} WHERE s_key IN $keys;");
    let Ok(mut response) = db.query(&query).bind(("keys", keys.to_vec())).await else {
        return;
    };
    let rows: Vec<JsonValue> = response.take(0).unwrap_or_default();
    for row in rows {
        if let (Some(key), Some(value)) = (row.get("s_key").and_then(JsonValue::as_str), row.get("s_value")) {
            out.insert(key.to_string(), value.clone());
        }
    }
}

/// The value of each key from an organization's session and (optionally) the core session: global first,
/// then the organization's own on top.
pub async fn effective_in(org: &crate::state::Db, core: Option<&crate::state::Db>, keys: &[&str]) -> HashMap<String, JsonValue> {
    let keys: Vec<String> = keys.iter().map(|k| (*k).to_string()).collect();
    let mut out = HashMap::new();
    if let Some(core) = core {
        read_into(core, "gl_settings_items", &keys, &mut out).await;
    }
    read_into(org, "settings_items", &keys, &mut out).await;
    out
}

/// The value of each key: global first, then the organization's own on top. A key nobody has set is
/// absent, so the caller's default applies.
pub async fn effective_settings(ctx: &PluginHostContext, keys: &[&str]) -> HashMap<String, JsonValue> {
    let core = ctx.services().ok().and_then(|services| services.core.clone());
    effective_in(&ctx.db, core.as_ref(), keys).await
}
