//! Built-in record actions every list and detail page offers: delete, archive and unarchive.
//!
//! They are reserved plugin function names (`kernel.delete_records`, `kernel.archive_records`,
//! `kernel.unarchive_records`) handled by the kernel itself, not by the plugin's code. They run
//! through the plugin's host context, so the caller's model grants, the model's `rules/` and the
//! data-access audit apply exactly as if the plugin had made the same `db::delete` / `db::update`
//! call: a model whose rules do not allow `delete` refuses it, per record, with the reason.
//!
//! Payload: `{ "model": "po", "ids": ["…"] }`. Answer: `{ "done": [...], "failed": [{ "id", "error" }] }`.
//! Archiving sets the model's `active` field (a bool); a model without one cannot be archived.

use serde_json::{Value, json};

use crate::kernel::{PluginHostContext, kernel_command};

/// Most records one action may touch.
const MAX_IDS: usize = 200;

pub const DELETE: &str = "kernel.delete_records";
pub const ARCHIVE: &str = "kernel.archive_records";
pub const UNARCHIVE: &str = "kernel.unarchive_records";

/// `Some(answer)` when `function` is one of the built-in record actions, `None` otherwise.
pub async fn run(
    host: &PluginHostContext,
    function: &str,
    payload: &Value,
) -> Option<Result<Value, String>> {
    let archived = match function {
        DELETE => None,
        ARCHIVE => Some(true),
        UNARCHIVE => Some(false),
        _ => return None,
    };
    Some(apply(host, payload, archived).await)
}

async fn apply(
    host: &PluginHostContext,
    payload: &Value,
    archived: Option<bool>,
) -> Result<Value, String> {
    let model = payload
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .ok_or("name the model of the records")?;
    let ids: Vec<&str> = payload
        .get("ids")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if ids.is_empty() {
        return Err("select at least one record".into());
    }
    if ids.len() > MAX_IDS {
        return Err(format!("an action takes at most {MAX_IDS} records at a time"));
    }
    let mut done = Vec::new();
    let mut failed = Vec::new();
    for id in ids {
        let outcome = match archived {
            None => kernel_command(host, "db::delete", json!({ "model": model, "id": id })).await,
            Some(flag) => {
                kernel_command(
                    host,
                    "db::update",
                    json!({ "model": model, "id": id, "data": { "active": !flag } }),
                )
                .await
            }
        };
        match outcome {
            Ok(_) => done.push(json!(id)),
            Err(error) => failed.push(json!({ "id": id, "error": error.to_string() })),
        }
    }
    Ok(json!({ "done": done, "failed": failed }))
}
