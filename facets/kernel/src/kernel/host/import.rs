//! `import::*` and `export::run`: records in from a spreadsheet file, and out to one (see
//! `docs/architecture/import_export.md`).
//!
//! An import is a job. `import::start` checks what can be checked at once (the model, the file, the
//! mapping) and queues it; the kernel then reads the file and applies its rows **as the person who
//! started it**, through the calling plugin, so the model's checks and that person's record and field
//! rules apply to every row. `mode` is `dry_run` (the default: every write is checked for real and taken
//! back, and the report says what a real run would do) or `run`. The file is an uploaded media file
//! (`media_key`) or a file in the plugin's own storage (`storage_key`).
//!
//! | Command | Does |
//! |---|---|
//! | `import::start { model, media_key \| storage_key, mode?, mapping?, sheet?, stop_on_error?, force?, on_done?, ref? }` | queue an import; answers `{ job }` |
//! | `import::status { job }` | the job: state, counts, the mapping used, what was guessed |
//! | `import::result { job, offset?, limit?, only_errors? }` | what became of each row |
//! | `import::undo { job }` | take back a finished real import |
//! | `import::templates { model }` | a CSV header row for the model, and its fields |
//! | `import::guess { model, headers }` | the mapping the kernel would guess |
//! | `export::run { model, filter?, fields?, format?, include_id? }` | write the records to a CSV or JSON file in the plugin's storage |

use aether_storage::{MediaKey, StorageError};
use bytes::Bytes;
use serde::Deserialize;
use serde_json::{Map, Value as JsonValue, json};

use super::context::PluginHostContext;
use super::db::{parse_req, require_model};
use super::error::HostError;
use crate::import::job;
use crate::import::mapping::{Mapping, guess, importable};

const MAX_RESULT_ROWS: u32 = 500;
const DEFAULT_EXPORT_ROWS: u64 = 50_000;
const EXPORT_PAGE: u32 = 500;

fn fail(message: impl Into<String>) -> HostError {
    HostError::Message(message.into())
}

/// A file the plugin names: `(kind, key)` and the key as the media backend knows it.
fn source(ctx: &PluginHostContext, media_key: Option<&str>, storage_key: Option<&str>) -> Result<((String, String), MediaKey), HostError> {
    match (media_key, storage_key) {
        (Some(key), None) => {
            let full = MediaKey::parse(key).map_err(|e| HostError::InvalidPayload(e.to_string()))?;
            Ok((("media".into(), key.to_string()), full))
        }
        (None, Some(key)) => {
            let full = super::storage::plugin_key(ctx, key)?;
            Ok((("storage".into(), key.to_string()), full))
        }
        _ => Err(HostError::InvalidPayload("name the file with `media_key` or `storage_key`, one of them".into())),
    }
}

async fn core_of(ctx: &PluginHostContext) -> Option<crate::databases::Db> {
    ctx.services.as_ref().and_then(|services| services.core.clone())
}

#[derive(Deserialize)]
struct StartRequest {
    model: String,
    #[serde(default)]
    media_key: Option<String>,
    #[serde(default)]
    storage_key: Option<String>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    mapping: Option<JsonValue>,
    #[serde(default)]
    sheet: Option<String>,
    #[serde(default)]
    stop_on_error: bool,
    #[serde(default)]
    force: bool,
    #[serde(default)]
    on_done: Option<String>,
    #[serde(default, rename = "ref")]
    reference: Option<String>,
}

/// `import::start`.
pub async fn import_start(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("import::run")?;
    let req: StartRequest = parse_req(payload)?;
    require_model(ctx, &req.model, true)?;
    let dry_run = match req.mode.as_deref() {
        None | Some("dry_run") => true,
        Some("run") => false,
        Some(other) => return Err(HostError::InvalidPayload(format!("`mode` is dry_run or run, not `{other}`"))),
    };
    // The import is done as a person; a visitor or the kernel itself cannot start one.
    let user = match (&ctx.audit.actor, ctx.audit.actor.id()) {
        (crate::access::audit::Actor::User(_), Some(id)) => id.to_string(),
        _ => return Err(HostError::Denied("only a signed-in person can start an import".into())),
    };
    if let Some(given) = &req.mapping {
        serde_json::from_value::<Mapping>(given.clone()).map_err(|e| HostError::InvalidPayload(format!("mapping: {e}")))?;
    }
    let (kind_key, full) = source(ctx, req.media_key.as_deref(), req.storage_key.as_deref())?;
    let services = ctx.services()?;
    let core = core_of(ctx).await;
    let cfg = job::settings(&ctx.db, core.as_ref()).await;
    match services.media.head(&full).await {
        Ok(object) if object.size > cfg.max_bytes => {
            return Err(fail(format!("files over {} MB are not imported", cfg.max_bytes / (1024 * 1024))));
        }
        Ok(_) => {}
        Err(StorageError::NotFound(_)) => return Err(fail("that file does not exist")),
        Err(error) => {
            log::error!("import: storage in `{}`: {error}", ctx.database);
            return Err(fail("storage is unavailable"));
        }
    }
    if !dry_run {
        let values = super::settings_read::effective_settings(ctx, &["import.dry_run_required"]).await;
        if values.get("import.dry_run_required") == Some(&JsonValue::Bool(true)) && !req.force {
            let mut response = ctx
                .db
                .query(
                    "SELECT errors FROM import_jobs WHERE plugin = $plugin AND model = $model AND user = $user AND mode = 'dry_run' \
                     AND status = 'done' AND source_key = $key AND created_at > time::now() - 1d ORDER BY created_at DESC LIMIT 1;",
                )
                .bind(("plugin", ctx.plugin_name.clone()))
                .bind(("model", req.model.clone()))
                .bind(("user", user.clone()))
                .bind(("key", kind_key.1.clone()))
                .await?
                .check()?;
            let tried: Vec<JsonValue> = response.take(0)?;
            if tried.first().and_then(|r| r["errors"].as_i64()) != Some(0) {
                return Err(fail("this organization requires a clean dry run of this file in the last day before a real import"));
            }
        }
    }
    let callback = req.on_done.as_ref().map(|function| json!({ "plugin": ctx.plugin_name, "function": function, "ref": req.reference }));
    let ulid = job::create(
        &ctx.db,
        job::Request {
            plugin: ctx.plugin_name.clone(),
            model: req.model.clone(),
            user,
            dry_run,
            source: kind_key,
            sheet: req.sheet,
            mapping: req.mapping,
            stop_on_error: req.stop_on_error,
            force: req.force,
            callback,
        },
    )
    .await?;
    if let Some(scheduler) = ctx.services.as_ref().and_then(|s| s.scheduler.clone()) {
        scheduler.wake(&ctx.database);
    }
    Ok(json!({ "ok": true, "data": { "job": ulid, "status": "queued", "mode": if dry_run { "dry_run" } else { "run" } } }))
}

#[derive(Deserialize)]
struct JobRequest {
    job: String,
}

/// The job, if this plugin started it for the person asking: an import is one person's act and report,
/// and what it says about rows can include data that others may not see.
async fn own_job(ctx: &PluginHostContext, ulid: &str) -> Result<JsonValue, HostError> {
    match job::find(&ctx.db, ulid).await? {
        Some(row) if row["plugin"].as_str() == Some(ctx.plugin_name.as_str()) && row["user"].as_str() == ctx.audit.actor.id() => Ok(row),
        _ => Err(fail("no such import")),
    }
}

fn summary(row: &JsonValue) -> JsonValue {
    let mut out = row.clone();
    if let Some(map) = out.as_object_mut() {
        map.remove("id");
        map.remove("callback");
        map.remove("force");
    }
    out
}

/// `import::status`.
pub async fn import_status(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("import::run")?;
    let req: JobRequest = parse_req(payload)?;
    Ok(json!({ "ok": true, "data": summary(&own_job(ctx, &req.job).await?) }))
}

#[derive(Deserialize)]
struct ResultRequest {
    job: String,
    #[serde(default)]
    offset: Option<u32>,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    only_errors: bool,
}

/// `import::result`.
pub async fn import_result(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("import::run")?;
    let req: ResultRequest = parse_req(payload)?;
    let row = own_job(ctx, &req.job).await?;
    let limit = req.limit.unwrap_or(100).clamp(1, MAX_RESULT_ROWS);
    let offset = req.offset.unwrap_or(0);
    let filter = if req.only_errors { " AND action = 'error'" } else { "" };
    let mut response = ctx
        .db
        .query(format!(
            "SELECT row, action, record_id, message, problems, before, after, undo FROM import_rows \
             WHERE job = $job{filter} ORDER BY row LIMIT $limit START $offset;"
        ))
        .bind(("job", req.job.clone()))
        .bind(("limit", limit))
        .bind(("offset", offset))
        .await?
        .check()?;
    let rows: Vec<JsonValue> = response.take(0)?;
    let next = (rows.len() == limit as usize).then_some(offset + limit);
    Ok(json!({ "ok": true, "data": { "job": summary(&row), "rows": rows, "next_offset": next } }))
}

/// `import::undo`.
pub async fn import_undo(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("import::run")?;
    let req: JobRequest = parse_req(payload)?;
    let row = own_job(ctx, &req.job).await?;
    if ctx.audit.actor.id() != row["user"].as_str() {
        return Err(HostError::Denied("only the person who ran the import can undo it".into()));
    }
    if row["mode"].as_str() != Some("run") {
        return Err(fail("a dry run changed nothing, so there is nothing to undo"));
    }
    if row["status"].as_str() != Some("done") {
        return Err(fail("only a finished import can be undone"));
    }
    if !row["undo"].is_null() {
        return Err(fail("this import was already undone"));
    }
    if row["created"].as_i64().unwrap_or(0) + row["updated"].as_i64().unwrap_or(0) == 0 {
        return Err(fail("the import changed nothing, so there is nothing to undo"));
    }
    ctx.db
        .query("UPDATE import_jobs SET status = 'undoing', undo = { status: 'running' } WHERE ulid = $ulid;")
        .bind(("ulid", req.job.clone()))
        .await?
        .check()?;
    job::enqueue(&ctx.db, &req.job, "undo", 0).await?;
    if let Some(scheduler) = ctx.services.as_ref().and_then(|s| s.scheduler.clone()) {
        scheduler.wake(&ctx.database);
    }
    Ok(json!({ "ok": true, "data": { "job": req.job, "status": "undoing" } }))
}

fn model_def_fields(ctx: &PluginHostContext, model: &str) -> Result<Vec<crate::data_model::FieldDef>, HostError> {
    let grant = require_model(ctx, model, false)?;
    let schema = grant
        .schema
        .as_deref()
        .ok_or_else(|| HostError::InvalidPayload(format!("`{model}` has no model definition")))?;
    Ok(schema.fields().cloned().collect())
}

/// `import::templates`.
pub async fn import_templates(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("import::run")?;
    let req: ModelRequest = parse_req(payload)?;
    let fields: Vec<_> = model_def_fields(ctx, &req.model)?.into_iter().filter(importable).collect();
    let header: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
    let described: Vec<JsonValue> = fields
        .iter()
        .map(|f| {
            json!({
                "name": f.name,
                "label": f.label,
                "type": serde_json::to_value(f.kind).unwrap_or(JsonValue::Null),
                "required": f.required && f.default.is_none(),
                "choices": f.options.iter().map(|o| o.value.clone()).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(json!({ "ok": true, "data": { "csv": format!("{}\n", header.join(",")), "fields": described } }))
}

#[derive(Deserialize)]
struct ModelRequest {
    model: String,
}

#[derive(Deserialize)]
struct GuessRequest {
    model: String,
    headers: Vec<String>,
}

/// `import::guess`: the mapping the kernel would propose for these headers.
pub async fn import_guess(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("import::run")?;
    let req: GuessRequest = parse_req(payload)?;
    if req.headers.len() > 200 {
        return Err(HostError::InvalidPayload("at most 200 headers".into()));
    }
    let model = crate::data_model::ModelDef {
        fields: model_def_fields(ctx, &req.model)?,
        ..serde_json::from_value(json!({ "name": req.model, "fields": [] })).map_err(|e| fail(e.to_string()))?
    };
    let guessed = guess(&req.headers, &model, &std::collections::BTreeMap::new());
    Ok(json!({ "ok": true, "data": serde_json::to_value(guessed).map_err(|e| fail(e.to_string()))? }))
}

#[derive(Deserialize)]
struct ExportRequest {
    model: String,
    #[serde(default)]
    filter: Map<String, JsonValue>,
    #[serde(default)]
    fields: Option<Vec<String>>,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    include_id: bool,
}

fn csv_cell(value: &JsonValue) -> String {
    let text = match value {
        JsonValue::Null => String::new(),
        JsonValue::String(s) => s.clone(),
        JsonValue::Bool(b) => b.to_string(),
        JsonValue::Number(n) => n.to_string(),
        other => other.to_string(),
    };
    if text.contains([',', '"', '\n', '\r']) || text.starts_with(' ') || text.ends_with(' ') {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text
    }
}

/// `export::run`: the records the caller may see, written to a file in the plugin's storage.
///
/// Values are written as stored (ISO dates, decimals as plain text), so the file can be imported again;
/// field and record rules apply exactly as for a read. At most `export.max_rows` records (50 000).
pub async fn export_run(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("export::run")?;
    let req: ExportRequest = parse_req(payload)?;
    let format = req.format.as_deref().unwrap_or("csv");
    if !matches!(format, "csv" | "json") {
        return Err(HostError::InvalidPayload("`format` is csv or json".into()));
    }
    let defs = model_def_fields(ctx, &req.model)?;
    let names: Vec<String> = match &req.fields {
        Some(list) => {
            for name in list {
                if !defs.iter().any(|f| &f.name == name) {
                    return Err(HostError::InvalidPayload(format!("`{name}` is not a field of `{}`", req.model)));
                }
            }
            list.clone()
        }
        None => defs
            .iter()
            .filter(|f| !f.deprecated && !matches!(f.kind, crate::data_model::FieldType::Many2many | crate::data_model::FieldType::Child))
            .map(|f| f.name.clone())
            .collect(),
    };
    let max = super::settings_read::effective_settings(ctx, &["export.max_rows"])
        .await
        .get("export.max_rows")
        .and_then(JsonValue::as_u64)
        .unwrap_or(DEFAULT_EXPORT_ROWS);

    // Exporting is allowed by `export::run`; the reading it does needs no other capability of the plugin.
    let mut reader = ctx.clone();
    reader.granted_capabilities.insert("db::query".into());
    let mut rows: Vec<JsonValue> = Vec::new();
    let mut after: Option<String> = None;
    loop {
        let mut filter = req.filter.clone();
        if let Some(last) = &after {
            let page = json!({ "id": { "gt": last } });
            filter = if filter.is_empty() { page.as_object().cloned().unwrap_or_default() } else { Map::from_iter([("and".to_string(), json!([JsonValue::Object(filter), page]))]) };
        }
        let reply = super::db::db_find(&reader, &json!({ "model": req.model, "filter": filter, "limit": EXPORT_PAGE, "order": "id" })).await?;
        let page = reply.get("data").and_then(JsonValue::as_array).cloned().unwrap_or_default();
        let done = page.len() < EXPORT_PAGE as usize;
        after = page.last().and_then(|r| r.get("id")).and_then(JsonValue::as_str).map(str::to_string);
        rows.extend(page);
        if rows.len() as u64 > max {
            return Err(fail(format!("more than {max} records match; narrow the filter")));
        }
        if done || after.is_none() {
            break;
        }
    }

    let (bytes, extension): (Vec<u8>, &str) = if format == "json" {
        let shaped: Vec<JsonValue> = rows
            .iter()
            .map(|row| {
                let mut out = Map::new();
                if req.include_id {
                    out.insert("id".into(), row.get("id").cloned().unwrap_or(JsonValue::Null));
                }
                for name in &names {
                    out.insert(name.clone(), row.get(name).cloned().unwrap_or(JsonValue::Null));
                }
                JsonValue::Object(out)
            })
            .collect();
        (serde_json::to_vec(&shaped).map_err(|e| fail(e.to_string()))?, "json")
    } else {
        let mut text = String::from('\u{feff}');
        let header: Vec<String> = req.include_id.then(|| "id".to_string()).into_iter().chain(names.iter().cloned()).collect();
        text.push_str(&header.join(","));
        text.push('\n');
        for row in &rows {
            let cells: Vec<String> = req
                .include_id
                .then(|| csv_cell(row.get("id").unwrap_or(&JsonValue::Null)))
                .into_iter()
                .chain(names.iter().map(|name| csv_cell(row.get(name).unwrap_or(&JsonValue::Null))))
                .collect();
            text.push_str(&cells.join(","));
            text.push('\n');
        }
        (text.into_bytes(), "csv")
    };
    let mut random = [0u8; 8];
    rand::RngExt::fill(&mut rand::rng(), &mut random);
    let name = format!("exports/{}-{}.{extension}", req.model, random.iter().map(|b| format!("{b:02x}")).collect::<String>());
    let key = super::storage::plugin_key(ctx, &name)?;
    let size = bytes.len();
    ctx.services()?.media.put(&key, Bytes::from(bytes)).await.map_err(|e| {
        log::error!("export: storage in `{}`: {e}", ctx.database);
        fail("storage is unavailable")
    })?;
    Ok(json!({ "ok": true, "data": { "storage_key": name, "rows": rows.len(), "bytes": size, "format": extension } }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_are_quoted_when_they_need_it() {
        assert_eq!(csv_cell(&json!("plain")), "plain");
        assert_eq!(csv_cell(&json!("a,b")), "\"a,b\"");
        assert_eq!(csv_cell(&json!("say \"x\"")), "\"say \"\"x\"\"\"");
        assert_eq!(csv_cell(&json!(null)), "");
        assert_eq!(csv_cell(&json!(true)), "true");
        assert_eq!(csv_cell(&json!(3.5)), "3.5");
        assert_eq!(csv_cell(&json!({ "a": 1 })), "\"{\"\"a\"\":1}\"");
    }
}
