//! An import as a scheduler job: slices, progress, the file of failed rows, undo. The row store and
//! settings are in `aether_kernel::import::job`.

pub use aether_kernel::import::job::*;

use std::time::Instant;
use aether_storage::MediaKey;
use bytes::Bytes;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use super::mapping::{Mapping, guess};
use super::rows::{self, Action, LinkCache, Outcome};
use super::table::{self, Limits, Row, Table};
use crate::scheduler::queue::{self, Job, NewJob};
use crate::state::{AppState, Db};

fn nulls_out(value: &mut Value) {
    if let Some(map) = value.as_object_mut() {
        map.retain(|_, v| !v.is_null());
    }
}

/// Keep what became of some rows.
async fn store_rows(db: &Db, ulid: &str, outcomes: &[Outcome]) -> Result<(), surrealdb::Error> {
    if outcomes.is_empty() {
        return Ok(());
    }
    let rows: Vec<Value> = outcomes
        .iter()
        .map(|o| {
            let mut value = serde_json::to_value(o).unwrap_or(Value::Null);
            nulls_out(&mut value);
            if let Some(map) = value.as_object_mut() {
                if map.get("problems").and_then(Value::as_array).is_some_and(Vec::is_empty) {
                    map.remove("problems");
                }
            }
            value
        })
        .collect();
    db.query(
        "FOR $o IN $rows { UPSERT import_rows SET job = $job, row = $o.row, action = $o.action, record_id = $o.record_id, \
         message = $o.message, problems = $o.problems, before = $o.before, after = $o.after \
         WHERE job = $job AND row = $o.row; };",
    )
    .bind(("job", ulid.to_string()))
    .bind(("rows", rows))
    .await?
    .check()?;
    Ok(())
}

/// Counts of the rows kept so far, by what became of them.
async fn tally(db: &Db, ulid: &str) -> Result<Map<String, Value>, surrealdb::Error> {
    let mut response = db
        .query("SELECT action, count() AS n FROM import_rows WHERE job = $job GROUP BY action;")
        .bind(("job", ulid.to_string()))
        .await?
        .check()?;
    let rows: Vec<Value> = response.take(0)?;
    let mut out = Map::new();
    for row in rows {
        if let (Some(action), Some(n)) = (row.get("action").and_then(Value::as_str), row.get("n")) {
            out.insert(action.to_string(), n.clone());
        }
    }
    Ok(out)
}

async fn set_progress(db: &Db, ulid: &str, processed: usize) -> Result<(), surrealdb::Error> {
    let counts = tally(db, ulid).await?;
    let n = |k: &str| counts.get(k).and_then(Value::as_i64).unwrap_or(0);
    db.query("UPDATE import_jobs SET processed = $p, created = $c, updated = $u, unchanged = $s, errors = $e WHERE ulid = $ulid;")
        .bind(("p", processed as i64))
        .bind(("c", n("create")))
        .bind(("u", n("update")))
        .bind(("s", n("unchanged")))
        .bind(("e", n("error")))
        .bind(("ulid", ulid.to_string()))
        .await?
        .check()?;
    Ok(())
}

/// Say the job cannot go on, and tell the plugin that asked.
async fn fail(state: &AppState, db: &Db, org_db: &str, ulid: &str, message: &str) {
    let _ = db
        .query("UPDATE import_jobs SET status = 'failed', error = $e, finished_at = time::now() WHERE ulid = $ulid;")
        .bind(("e", message.to_string()))
        .bind(("ulid", ulid.to_string()))
        .await;
    call_back(&state.scheduler, db, org_db, ulid).await;
}

/// Tell the plugin function that asked, if one did, how the job ended.
async fn call_back(scheduler: &crate::scheduler::SchedulerLink, db: &Db, org_db: &str, ulid: &str) {
    let Ok(Some(job)) = find(db, ulid).await else {
        return;
    };
    let Some(callback) = job.get("callback").filter(|c| c.is_object()) else {
        return;
    };
    let (Some(plugin), Some(function)) = (callback.get("plugin").and_then(Value::as_str), callback.get("function").and_then(Value::as_str)) else {
        return;
    };
    let mut queued = NewJob::new("plugin", "default");
    queued.plugin = Some(plugin.to_string());
    queued.function = Some(function.to_string());
    queued.payload = Some(json!({
        "event": "import.done",
        "job": ulid,
        "status": job.get("status"),
        "mode": job.get("mode"),
        "model": job.get("model"),
        "created": job.get("created"),
        "updated": job.get("updated"),
        "unchanged": job.get("unchanged"),
        "errors": job.get("errors"),
        "error": job.get("error"),
        "errors_key": job.get("errors_key"),
        "ref": callback.get("ref"),
    }));
    queued.enqueued_by = Some("system:import".into());
    if let Err(error) = queue::enqueue(db, queued).await {
        log::warn!("{org_db}: the import callback to `{plugin}.{function}` could not be queued: {error}");
    }
    scheduler.wake(org_db);
}

fn csv_cell(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) || text.starts_with(' ') || text.ends_with(' ') {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

/// The rows that failed, as a file to fix and import again: the original cells and the reason.
pub fn error_file(table: &Table, errors: &[(usize, String)]) -> String {
    let mut out = String::from('\u{feff}');
    let header: Vec<String> = table.headers.iter().map(|h| csv_cell(h)).chain(std::iter::once("_error".to_string())).collect();
    out.push_str(&header.join(","));
    out.push('\n');
    for (number, message) in errors {
        if let Some(row) = table.rows.iter().find(|r| r.number == *number) {
            let cells: Vec<String> = row.cells.iter().map(|c| csv_cell(c)).chain(std::iter::once(csv_cell(message))).collect();
            out.push_str(&cells.join(","));
            out.push('\n');
        }
    }
    out
}

fn limits_of(s: &Settings) -> Limits {
    Limits { max_rows: s.max_rows, ..Limits::default() }
}

async fn read_file(media: &crate::state::Media, org_db: &str, job: &Value, max_bytes: u64) -> Result<Bytes, Failure> {
    let plugin = job.get("plugin").and_then(Value::as_str).unwrap_or_default();
    let kind = job.get("source_kind").and_then(Value::as_str).unwrap_or_default();
    let key = job.get("source_key").and_then(Value::as_str).unwrap_or_default();
    let full = if kind == "storage" { format!("plugins/{plugin}/{key}") } else { key.to_string() };
    let media_key = MediaKey::parse(&full).map_err(|e| (e.to_string(), true))?;
    let media = media.org(org_db).map_err(|e| (format!("storage is unavailable ({e})"), false))?;
    match media.head(&media_key).await {
        Ok(object) if object.size > max_bytes => {
            return Err((format!("files over {} MB are not imported", max_bytes / (1024 * 1024)), true));
        }
        Ok(_) => {}
        Err(aether_storage::StorageError::NotFound(_)) => return Err(("that file does not exist".into(), true)),
        Err(error) => return Err((format!("storage is unavailable ({error})"), false)),
    }
    media.get(&media_key).await.map_err(|e| (format!("storage is unavailable ({e})"), false))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Run a slice of a job.
pub async fn run(state: &AppState, org_db: &str, job: &Job) -> Result<(), Failure> {
    let payload = job.payload.clone().unwrap_or(Value::Null);
    let ulid = payload["job"].as_str().ok_or(("the job names no import".to_string(), true))?.to_string();
    let phase = payload["phase"].as_str().unwrap_or("import").to_string();
    let db = state.org(org_db).await.map_err(|e| (format!("the database: {e}"), false))?;
    let Some(row) = find(&db, &ulid).await.map_err(|e| (format!("the database: {e}"), false))? else {
        return Ok(()); // purged meanwhile
    };
    let outcome = if phase == "undo" {
        undo_slice(state, org_db, &db, &row).await
    } else {
        import_slice(state, org_db, &db, &row).await
    };
    match outcome {
        Ok(()) => Ok(()),
        Err((message, permanent)) => {
            if permanent || job.attempts >= job.max_attempts {
                fail(state, &db, org_db, &ulid, &message).await;
                return Ok(());
            }
            Err((message, false))
        }
    }
}

async fn import_slice(state: &AppState, org_db: &str, db: &Db, row: &Value) -> Result<(), Failure> {
    let started = Instant::now();
    let ulid = row["ulid"].as_str().unwrap_or_default().to_string();
    let status = row["status"].as_str().unwrap_or_default();
    if !matches!(status, "queued" | "running") {
        return Ok(());
    }
    let dry = row["mode"].as_str() == Some("dry_run");
    let core = state.core().await.ok();
    let cfg = settings(db, core.as_ref()).await;
    let db_err = |e: surrealdb::Error| (format!("the database: {e}"), false);
    if status == "queued" {
        db.query("UPDATE import_jobs SET status = 'running', started_at = time::now() WHERE ulid = $ulid;")
            .bind(("ulid", ulid.clone()))
            .await
            .and_then(|r| r.check())
            .map_err(db_err)?;
    }

    let bytes = read_file(&state.media, org_db, row, cfg.max_bytes).await?;
    let hash = hex(&Sha256::digest(&bytes));
    let model_name = row["model"].as_str().unwrap_or_default().to_string();
    let plugin = row["plugin"].as_str().unwrap_or_default().to_string();
    let user = row["user"].as_str().unwrap_or_default().to_string();

    // The same file is not imported twice by accident.
    if row["file_hash"].is_null() {
        if !dry && row["force"].as_bool() != Some(true) {
            let mut response = db
                .query(
                    "SELECT ulid FROM import_jobs WHERE model = $model AND file_hash = $hash AND mode = 'run' \
                     AND status = 'done' AND undo = NONE AND ulid != $ulid LIMIT 1;",
                )
                .bind(("model", model_name.clone()))
                .bind(("hash", hash.clone()))
                .bind(("ulid", ulid.clone()))
                .await
                .and_then(|r| r.check())
                .map_err(db_err)?;
            let earlier: Vec<Value> = response.take(0).map_err(db_err)?;
            if let Some(other) = earlier.first().and_then(|r| r["ulid"].as_str()) {
                return Err((format!("this file was already imported (import {other}); send `force` to import it again"), true));
            }
        }
        db.query("UPDATE import_jobs SET file_hash = $hash WHERE ulid = $ulid;")
            .bind(("hash", hash))
            .bind(("ulid", ulid.clone()))
            .await
            .and_then(|r| r.check())
            .map_err(db_err)?;
    }

    let table = table::parse(&bytes, row["sheet"].as_str(), limits_of(&cfg)).map_err(|e| (e.to_string(), true))?;
    let request_id = format!("import-{ulid}");
    let (loaded, host) = crate::plugin_manager::api::host_for_user(state, org_db, &user, &plugin, &format!("import:{model_name}"), &request_id)
        .await
        .map_err(|e| (e.message, e.permanent))?;
    let model = loaded
        .models
        .iter()
        .find(|m| m.name == model_name)
        .ok_or_else(|| (format!("`{plugin}` has no model `{model_name}`"), true))?;
    let mut host = host;
    host.granted_capabilities.insert("db::query".into());
    host.granted_capabilities.insert("db::mutate".into());
    if dry {
        host = host.with_dry_run();
    }

    // The mapping: the one already applied (a later slice), the person's, or a guess from the headers.
    let mapping: Mapping = if let Some(used) = row["mapping_used"].as_object() {
        serde_json::from_value(Value::Object(used.clone())).map_err(|e| (format!("the mapping: {e}"), true))?
    } else if let Some(given) = row["mapping"].as_object() {
        serde_json::from_value(Value::Object(given.clone())).map_err(|e| (format!("the mapping: {e}"), true))?
    } else {
        let guessed = guess(&table.headers, model, &std::collections::BTreeMap::new());
        db.query("UPDATE import_jobs SET guessed = $g WHERE ulid = $ulid;")
            .bind(("g", json!({ "matched": guessed.matched, "unmatched_headers": guessed.unmatched_headers, "missing_required": guessed.missing_required })))
            .bind(("ulid", ulid.clone()))
            .await
            .and_then(|r| r.check())
            .map_err(db_err)?;
        guessed.mapping
    };
    if row["mapping_used"].is_null() {
        db.query("UPDATE import_jobs SET mapping_used = $m, headers = $h, total_rows = $t, format = $f WHERE ulid = $ulid;")
            .bind(("m", serde_json::to_value(&mapping).unwrap_or(Value::Null)))
            .bind(("h", table.headers.clone()))
            .bind(("t", table.rows.len() as i64))
            .bind(("f", table.format.name().to_string()))
            .bind(("ulid", ulid.clone()))
            .await
            .and_then(|r| r.check())
            .map_err(db_err)?;
    }
    let resolved = rows::resolve(model, &table.headers, &mapping).map_err(|e| (e, true))?;

    let stop = row["stop_on_error"].as_bool() == Some(true);
    let mut processed = row["processed"].as_u64().unwrap_or(0) as usize;
    let mut links = LinkCache::default();
    while processed < table.rows.len() {
        let end = (processed + cfg.batch).min(table.rows.len());
        let slice: &[Row] = &table.rows[processed..end];
        let outcomes = rows::apply_batch(&host, &resolved, slice, stop, &mut links).await;
        let stopped = stop && outcomes.last().is_some_and(|o| o.action == Action::Error);
        store_rows(db, &ulid, &outcomes).await.map_err(db_err)?;
        processed += outcomes.len();
        set_progress(db, &ulid, processed).await.map_err(db_err)?;
        if stopped {
            break;
        }
        if started.elapsed() > SLICE && processed < table.rows.len() {
            enqueue(db, &ulid, "import", 0).await.map_err(db_err)?;
            state.scheduler.wake(org_db);
            return Ok(());
        }
    }

    // Finished: the exact counts, the file of rows to fix, and the word to the plugin.
    set_progress(db, &ulid, processed).await.map_err(db_err)?;
    let mut response = db
        .query("SELECT row, message FROM import_rows WHERE job = $job AND action = 'error' ORDER BY row;")
        .bind(("job", ulid.clone()))
        .await
        .and_then(|r| r.check())
        .map_err(db_err)?;
    let failed: Vec<Value> = response.take(0).map_err(db_err)?;
    let failed: Vec<(usize, String)> = failed
        .iter()
        .filter_map(|r| Some((r["row"].as_u64()? as usize, r["message"].as_str().unwrap_or_default().to_string())))
        .collect();
    let mut errors_key = None;
    if !failed.is_empty() {
        let key = format!("imports/{ulid}.errors.csv");
        if let (Ok(media), Ok(full)) = (state.org_media(org_db), MediaKey::parse(&format!("plugins/{plugin}/{key}"))) {
            if media.put(&full, Bytes::from(error_file(&table, &failed))).await.is_ok() {
                errors_key = Some(key);
            }
        }
    }
    db.query("UPDATE import_jobs SET status = 'done', finished_at = time::now(), errors_key = $key WHERE ulid = $ulid;")
        .bind(("key", errors_key))
        .bind(("ulid", ulid.clone()))
        .await
        .and_then(|r| r.check())
        .map_err(db_err)?;
    call_back(&state.scheduler, db, org_db, &ulid).await;
    Ok(())
}

/// Take back a real import, a slice at a time: the records it created are deleted and the ones it
/// changed get their old values back, unless someone has changed them since (those are left alone and
/// said so).
async fn undo_slice(state: &AppState, org_db: &str, db: &Db, row: &Value) -> Result<(), Failure> {
    let started = Instant::now();
    let ulid = row["ulid"].as_str().unwrap_or_default().to_string();
    if row["status"].as_str() != Some("undoing") {
        return Ok(());
    }
    let db_err = |e: surrealdb::Error| (format!("the database: {e}"), false);
    let (model, plugin, user) = (
        row["model"].as_str().unwrap_or_default().to_string(),
        row["plugin"].as_str().unwrap_or_default().to_string(),
        row["user"].as_str().unwrap_or_default().to_string(),
    );
    let request_id = format!("import-undo-{ulid}");
    let (_, mut host) = crate::plugin_manager::api::host_for_user(state, org_db, &user, &plugin, &format!("import-undo:{model}"), &request_id)
        .await
        .map_err(|e| (e.message, e.permanent))?;
    host.granted_capabilities.insert("db::query".into());
    host.granted_capabilities.insert("db::mutate".into());
    let core = state.core().await.ok();
    let batch = settings(db, core.as_ref()).await.batch;

    loop {
        let mut response = db
            .query("SELECT * FROM import_rows WHERE job = $job AND action IN ['create', 'update'] AND undo = NONE ORDER BY row DESC LIMIT $n;")
            .bind(("job", ulid.clone()))
            .bind(("n", batch as i64))
            .await
            .and_then(|r| r.check())
            .map_err(db_err)?;
        let todo: Vec<Value> = response.take(0).map_err(db_err)?;
        if todo.is_empty() {
            break;
        }
        for entry in &todo {
            let result = undo_row(&host, &model, entry).await;
            db.query("UPDATE import_rows SET undo = $u WHERE job = $job AND row = $row;")
                .bind(("u", result))
                .bind(("job", ulid.clone()))
                .bind(("row", entry["row"].as_i64().unwrap_or(0)))
                .await
                .and_then(|r| r.check())
                .map_err(db_err)?;
        }
        if started.elapsed() > SLICE {
            enqueue(db, &ulid, "undo", 0).await.map_err(db_err)?;
            state.scheduler.wake(org_db);
            return Ok(());
        }
    }
    let mut response = db
        .query("SELECT undo, count() AS n FROM import_rows WHERE job = $job AND undo != NONE GROUP BY undo;")
        .bind(("job", ulid.clone()))
        .await
        .and_then(|r| r.check())
        .map_err(db_err)?;
    let groups: Vec<Value> = response.take(0).map_err(db_err)?;
    let (mut undone, mut skipped, mut failed) = (0, 0, 0);
    for group in &groups {
        let n = group["n"].as_i64().unwrap_or(0);
        match group["undo"].as_str().unwrap_or("") {
            "undone" => undone += n,
            s if s.starts_with("skipped") => skipped += n,
            _ => failed += n,
        }
    }
    db.query("UPDATE import_jobs SET status = 'undone', undo = $u, finished_at = time::now() WHERE ulid = $ulid;")
        .bind(("u", json!({ "status": "done", "undone": undone, "skipped": skipped, "failed": failed })))
        .bind(("ulid", ulid.clone()))
        .await
        .and_then(|r| r.check())
        .map_err(db_err)?;
    call_back(&state.scheduler, db, org_db, &ulid).await;
    Ok(())
}

fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}

/// Take back one row; the words for the row's `undo` column.
async fn undo_row(host: &crate::kernel::PluginHostContext, model: &str, entry: &Value) -> String {
    use crate::kernel::kernel_command;
    let Some(id) = entry["record_id"].as_str() else {
        return "skipped: the record is not known".into();
    };
    let current = match kernel_command(host, "db::get", json!({ "model": model, "id": id })).await {
        Ok(reply) => reply.get("data").filter(|d| !d.is_null()).cloned(),
        Err(error) => return format!("failed: {error}"),
    };
    let Some(current) = current else {
        return "skipped: the record is already gone".into();
    };
    let after = entry["after"].as_object().cloned().unwrap_or_default();
    if after.iter().any(|(field, value)| current.get(field).is_none_or(|now| !same(now, value))) {
        return "skipped: the record was changed after the import".into();
    }
    let result = if entry["action"].as_str() == Some("create") {
        kernel_command(host, "db::delete", json!({ "model": model, "id": id })).await
    } else {
        let before = entry["before"].as_object().cloned().unwrap_or_default();
        kernel_command(host, "db::update", json!({ "model": model, "id": id, "data": before })).await
    };
    match result {
        Ok(_) => "undone".into(),
        Err(error) => format!("failed: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::table::{Format, Row, Table};

    #[test]
    fn the_error_file_is_the_original_rows_with_the_reason_and_can_be_read_back() {
        let table = Table {
            headers: vec!["Name".into(), "Note".into()],
            rows: vec![
                Row { number: 2, cells: vec!["Ann".into(), "fine".into()] },
                Row { number: 3, cells: vec!["Bob, Jr.".into(), "said \"hi\"\nthere".into()] },
            ],
            format: Format::Csv { delimiter: ',' },
        };
        let file = error_file(&table, &[(3, "`Status` is not one of: new, won".into())]);
        assert!(file.starts_with('\u{feff}'), "a byte order mark, so a spreadsheet program reads it as UTF-8");
        let back = table::parse(file.as_bytes(), None, Limits::default()).expect("readable");
        assert_eq!(back.headers, vec!["Name", "Note", "_error"]);
        assert_eq!(back.rows.len(), 1, "only the failed row");
        assert_eq!(back.rows[0].cells, vec!["Bob, Jr.", "said \"hi\"\nthere", "`Status` is not one of: new, won"]);
    }

    #[test]
    fn cells_with_separators_or_quotes_are_quoted() {
        assert_eq!(csv_cell("plain"), "plain");
        assert_eq!(csv_cell("a,b"), "\"a,b\"");
        assert_eq!(csv_cell("say \"x\""), "\"say \"\"x\"\"\"");
        assert_eq!(csv_cell(" padded"), "\" padded\"");
    }

    #[test]
    fn numbers_are_the_same_by_value() {
        assert!(same(&json!(2), &json!(2.0)));
        assert!(!same(&json!("2"), &json!(2)));
    }
}
