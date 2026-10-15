//! Removes uploaded files that no record holds any more.
//!
//! A file ends up unreferenced when an upload is abandoned (a form that was never saved), or
//! when a picture or attachment is replaced or removed. Left alone they would pile up in the
//! organization's storage for ever.
//!
//! Every few hours, for every organization:
//!
//! 1. collect every file key held by an `image` or `file` field of every installed plugin's
//!    models (enabled or not: a disabled plugin's data is still data);
//! 2. list the organization's `media/` folder;
//! 3. delete what is in the folder, is older than [`GRACE`], and is not in the first list.
//!
//! The grace period is what keeps a file uploaded into a form that is still open safe. Any
//! doubt (a table that cannot be read, an object whose age the backend does not give)
//! keeps the file: the pass for that organization is skipped, never guessed.

use std::collections::HashSet;
use std::time::Duration;

use aether_storage::MediaKey;
use serde_json::Value;

use crate::data_model::media::{MEDIA_FOLDER, is_media_key};
use crate::plugin_manager::{installed, surface};
use crate::state::AppState;

/// How long an unreferenced file is kept.
pub const GRACE: Duration = Duration::from_secs(7 * 24 * 3600);
const INTERVAL: Duration = Duration::from_secs(6 * 3600);
/// Time after start-up before the first pass, so it does not compete with start-up work.
const FIRST_PASS_DELAY: Duration = Duration::from_secs(10 * 60);
/// The sidecar a picture's list thumbnail is stored under (`<key>.thumb`).
pub const THUMB_SUFFIX: &str = ".thumb";

/// Collect the file keys in a field's value (one file, a list, or old text: no keys).
pub fn referenced_keys(value: &Value, out: &mut HashSet<String>) {
    match value {
        Value::Object(object) => {
            if let Some(key) = object.get("key").and_then(Value::as_str) {
                out.insert(key.to_string());
            }
        }
        Value::Array(items) => items.iter().for_each(|item| referenced_keys(item, out)),
        _ => {}
    }
}

/// `(table, column)` of every image and file field in the models: records are stored in
/// the table named by the model's id, under columns named by field ids.
pub fn media_columns(models: &[(String, Value)]) -> Vec<(String, String)> {
    let plain = |text: &str| {
        !text.is_empty() && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    let mut columns = Vec::new();
    for (_, definition) in models {
        let Some(table) = definition.get("model_id").and_then(Value::as_str) else {
            continue;
        };
        let Some(fields) = definition.get("fields").and_then(Value::as_array) else {
            continue;
        };
        for field in fields {
            let kind = field.get("type").and_then(Value::as_str);
            let id = field.get("id").and_then(Value::as_str);
            if let (Some("image" | "file"), Some(id)) = (kind, id)
                && plain(table)
                && plain(id)
            {
                columns.push((table.to_string(), id.to_string()));
            }
        }
    }
    columns
}

/// Which of the `objects` (`key`, last written time) to delete.
pub fn plan_deletions(
    objects: &[(String, Option<i64>)],
    referenced: &HashSet<String>,
    now: i64,
    grace: Duration,
) -> Vec<String> {
    let grace = i64::try_from(grace.as_secs()).unwrap_or(i64::MAX);
    objects
        .iter()
        .filter(|(key, _)| is_media_key(key))
        .filter(|(key, _)| {
            // A thumbnail lives and dies with its picture.
            let parent = key.strip_suffix(THUMB_SUFFIX).unwrap_or(key);
            !referenced.contains(parent)
        })
        // An object whose age is not known is kept.
        .filter(|(_, modified)| modified.is_some_and(|at| now.saturating_sub(at) >= grace))
        .map(|(key, _)| key.clone())
        .collect()
}

/// Run the cleanup now and then. Returns the task so shutdown can stop it.
pub fn spawn_cleanup(state: &AppState) -> tokio::task::JoinHandle<()> {
    let state = state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(FIRST_PASS_DELAY).await;
        loop {
            match cleanup(&state).await {
                Ok(0) => {}
                Ok(removed) => log::info!("Media cleanup removed {removed} unused file(s)"),
                Err(error) => log::warn!("Media cleanup failed: {error}"),
            }
            tokio::time::sleep(INTERVAL).await;
        }
    })
}

async fn cleanup(state: &AppState) -> Result<usize, String> {
    let organizations = crate::access::org_databases::list_cached(state)
        .await
        .map_err(|error| error.to_string())?;
    let mut removed = 0;
    for organization in organizations {
        match cleanup_organization(state, &organization).await {
            Ok(count) => removed += count,
            // One organization's trouble does not stop the others.
            Err(error) => log::warn!("Media cleanup skipped `{organization}`: {error}"),
        }
    }
    Ok(removed)
}

async fn cleanup_organization(state: &AppState, organization: &str) -> Result<usize, String> {
    let backend = state
        .org_media(organization)
        .map_err(|error| error.to_string())?;
    let folder = MediaKey::parse(MEDIA_FOLDER).map_err(|error| error.to_string())?;
    let objects: Vec<(String, Option<i64>)> = backend
        .list(Some(&folder))
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|object| (object.key, object.last_modified))
        .collect();
    if objects.is_empty() {
        return Ok(0);
    }

    let referenced = referenced_in(state, organization).await?;
    let now = chrono::Utc::now().timestamp();
    let doomed = plan_deletions(&objects, &referenced, now, GRACE);
    let mut removed = 0;
    for key in doomed {
        let media_key = MediaKey::parse(&key).map_err(|error| error.to_string())?;
        backend
            .delete(&media_key)
            .await
            .map_err(|error| error.to_string())?;
        // Thumbnails are removed with their picture, but only count files once.
        if !key.ends_with(THUMB_SUFFIX) {
            removed += 1;
        }
    }
    Ok(removed)
}

/// Every file key any record of the organization holds. An error anywhere is an error
/// here: a partial list must never be used to delete.
async fn referenced_in(state: &AppState, organization: &str) -> Result<HashSet<String>, String> {
    let org = state
        .org(organization)
        .await
        .map_err(|error| error.to_string())?;
    let core = state.core().await.map_err(|error| error.to_string())?;
    let plugins = installed::list_cached(&state.cache, organization, &org)
        .await
        .map_err(|error| error.to_string())?;

    let mut referenced = HashSet::new();
    for plugin in plugins {
        let models =
            surface::models_cached(&state.cache, &core, &plugin.plugin_name, &plugin.version)
                .await
                .map_err(|error| error.to_string())?;
        for (table, column) in media_columns(&models) {
            let mut response = org
                .query(format!(
                    "SELECT VALUE {column} FROM {table} WHERE {column} != NONE;"
                ))
                .await
                .and_then(|response| response.check())
                .map_err(|error| format!("{table}.{column}: {error}"))?;
            let values: Vec<Value> = response
                .take(0)
                .map_err(|error| format!("{table}.{column}: {error}"))?;
            values
                .iter()
                .for_each(|value| referenced_keys(value, &mut referenced));
        }
    }
    Ok(referenced)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const DAY: i64 = 24 * 3600;

    #[test]
    fn finds_keys_in_one_file_or_a_list() {
        let mut keys = HashSet::new();
        referenced_keys(&json!({ "key": "media/a.png", "name": "a" }), &mut keys);
        referenced_keys(
            &json!([{ "key": "media/b.pdf" }, { "key": "media/c.pdf" }, 5]),
            &mut keys,
        );
        referenced_keys(&json!("data:image/png;base64,AAAA"), &mut keys);
        referenced_keys(&Value::Null, &mut keys);
        let mut found: Vec<_> = keys.into_iter().collect();
        found.sort();
        assert_eq!(found, ["media/a.png", "media/b.pdf", "media/c.pdf"]);
    }

    #[test]
    fn picks_image_and_file_columns() {
        let models = vec![(
            "employee".to_string(),
            json!({ "model_id": "mdl_abc", "fields": [
                { "id": "fld_1", "name": "image", "type": "image" },
                { "id": "fld_2", "name": "name", "type": "string" },
                { "id": "fld_3", "name": "docs", "type": "file", "deprecated": true },
                { "id": "fld 4", "name": "bad", "type": "file" },
                { "name": "no_id", "type": "file" }
            ] }),
        )];
        assert_eq!(
            media_columns(&models),
            [
                ("mdl_abc".to_string(), "fld_1".to_string()),
                ("mdl_abc".to_string(), "fld_3".to_string())
            ]
        );
    }

    #[test]
    fn deletes_only_old_unreferenced_files() {
        let now = 100 * DAY;
        let objects = vec![
            ("media/2026/10/kept.png".to_string(), Some(now - 30 * DAY)),
            (
                "media/2026/10/kept.png.thumb".to_string(),
                Some(now - 30 * DAY),
            ),
            ("media/2026/10/old.pdf".to_string(), Some(now - 30 * DAY)),
            (
                "media/2026/10/old.pdf.thumb".to_string(),
                Some(now - 30 * DAY),
            ),
            ("media/2026/10/fresh.pdf".to_string(), Some(now - DAY)),
            ("media/2026/10/unknown.pdf".to_string(), None),
            ("plugins/hr/receipt.pdf".to_string(), Some(now - 30 * DAY)),
            (".aether-org".to_string(), Some(now - 30 * DAY)),
        ];
        let referenced: HashSet<String> = ["media/2026/10/kept.png".to_string()].into();
        let doomed = plan_deletions(&objects, &referenced, now, GRACE);
        assert_eq!(
            doomed,
            ["media/2026/10/old.pdf", "media/2026/10/old.pdf.thumb"]
        );
    }
}
