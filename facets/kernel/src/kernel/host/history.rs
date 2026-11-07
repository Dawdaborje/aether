//! Reading the past of a record: `db::get` with `as_of`, `history::list` and `history::diff`.
//!
//! The versions come from `record_versions` (written with every write of a model with `history`
//! on, see `docs/architecture/history.md`). What a caller may see of the past is decided by the
//! model's rules **as they are now** and the caller's roles now: a state is shown only if the
//! record, in that state, is one the caller may read today, and a field the caller cannot read now
//! is never shown for the past either. Every read is audited like any other read of the record.

use serde::Deserialize;
use serde_json::{Map, Value as JsonValue, json};

use super::context::PluginHostContext;
use super::db::{
    Access, Extra, FindRequest, RECORD_ID, decode, guarded_condition, parse_req, record_binds,
    require_model, run_audited,
};
use super::error::HostError;
use super::guard;
use crate::data_model::history::{self as past, Version};
use crate::data_model::{ModelSchema, Operation};

/// Versions one call reads at most, whatever span it asks for.
const MAX_SPAN: u64 = 1000;
const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 500;

/// The model, its schema and the bare record key a request names, when the model keeps history.
fn target<'a>(
    ctx: &'a PluginHostContext,
    model: &str,
    id: &str,
) -> Result<(&'a super::context::ModelGrant, &'a ModelSchema, String), HostError> {
    let grant = require_model(ctx, model, false)?;
    let schema = grant
        .schema
        .as_deref()
        .filter(|schema| schema.history.is_some())
        .ok_or_else(|| {
            HostError::InvalidPayload(format!("`{model}` does not keep a history (`history`)"))
        })?;
    let key = super::db::strip_table_prefix(id, &grant.table).to_string();
    Ok((grant, schema, key))
}

/// Read rows with the caller's own database handle (these are the kernel's tables).
async fn rows(
    ctx: &PluginHostContext,
    surql: &str,
    binds: Vec<(&'static str, JsonValue)>,
    index: usize,
) -> Result<Vec<JsonValue>, HostError> {
    let mut query = ctx.db.query(surql);
    for (name, value) in binds {
        query = query.bind((name, value));
    }
    let mut response = query.await.map_err(HostError::Db)?.check().map_err(HostError::Db)?;
    response.take(index).map_err(HostError::Db)
}

fn record_binds_for(table: &str, key: &str) -> Vec<(&'static str, JsonValue)> {
    vec![
        ("m", JsonValue::String(table.to_string())),
        ("k", JsonValue::String(key.to_string())),
    ]
}

/// The versions from the last snapshot at or before `lo` up to `hi`, in order.
async fn load(
    ctx: &PluginHostContext,
    table: &str,
    key: &str,
    lo: u64,
    hi: u64,
) -> Result<Vec<Version>, HostError> {
    if hi < lo || hi - lo > MAX_SPAN {
        return Err(HostError::InvalidPayload(format!(
            "ask for at most {MAX_SPAN} versions at a time"
        )));
    }
    let mut binds = record_binds_for(table, key);
    binds.push(("lo", json!(lo)));
    binds.push(("hi", json!(hi)));
    let found = rows(
        ctx,
        "LET $s = (SELECT VALUE seq FROM record_versions WHERE model_id = $m AND record_key = $k \
         AND seq <= $lo AND snapshot != NONE ORDER BY seq DESC LIMIT 1)[0] ?? 0;\
         SELECT * FROM record_versions WHERE model_id = $m AND record_key = $k \
         AND seq >= $s AND seq <= $hi ORDER BY seq;",
        binds,
        1,
    )
    .await?;
    Ok(found.iter().filter_map(Version::from_row).collect())
}

/// The states at these versions that the caller may read now, with their `seq`. One audited read.
async fn visible(
    ctx: &PluginHostContext,
    grant: &super::context::ModelGrant,
    id: &str,
    states: Vec<(u64, Map<String, JsonValue>)>,
) -> Result<Vec<(u64, JsonValue)>, HostError> {
    if states.is_empty() {
        return Ok(Vec::new());
    }
    let Some(scope) = guard::scope(ctx, grant, Operation::Read, "r").await? else {
        return Ok(Vec::new());
    };
    let objects: Vec<JsonValue> = states
        .into_iter()
        .map(|(seq, mut state)| {
            state.insert("__seq".into(), json!(seq));
            JsonValue::Object(state)
        })
        .collect();
    let mut binds = record_binds(grant, id);
    binds.push(("__states".to_string(), JsonValue::Array(objects)));
    binds.extend(scope.binds);
    let filter = if scope.sql.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", scope.sql)
    };
    let body = format!(
        "LET $rows = (SELECT *, type::record($__table, $__key) AS id FROM $__states{filter});"
    );
    let found = run_audited(
        ctx,
        Access {
            operation: "read",
            model: grant.name.clone(),
            table: &grant.table,
        },
        &body,
        RECORD_ID,
        binds,
        Extra::default(),
    )
    .await?;
    Ok(found
        .into_iter()
        .filter_map(|mut row| {
            let seq = row.as_object_mut()?.remove("__seq")?.as_u64()?;
            Some((seq, row))
        })
        .collect())
}

/// A stored-id state as the plugin sees it: names, without the fields the caller cannot read.
async fn shown(
    ctx: &PluginHostContext,
    grant: &super::context::ModelGrant,
    row: JsonValue,
) -> Result<JsonValue, HostError> {
    let (hidden, _) = guard::field_limits(ctx, grant).await?;
    Ok(guard::strip(&hidden, decode(grant, row)))
}

/// Names for stored field ids, for the fields the caller may read.
async fn names_of(
    ctx: &PluginHostContext,
    grant: &super::context::ModelGrant,
    schema: &ModelSchema,
    ids: &[String],
) -> Result<Vec<String>, HostError> {
    let (hidden, _) = guard::field_limits(ctx, grant).await?;
    let stored: Map<String, JsonValue> = ids.iter().map(|id| (id.clone(), JsonValue::Null)).collect();
    let named = schema.decode(&JsonValue::Object(stored));
    let mut names: Vec<String> = named
        .as_object()
        .map(|map| map.keys().filter(|name| !hidden.contains(*name)).cloned().collect())
        .unwrap_or_default();
    names.sort();
    Ok(names)
}

/// The version a moment falls in: a timestamp (RFC 3339) or `{ "seq": n }`.
enum AsOf {
    At(String),
    Seq(u64),
}

fn parse_as_of(value: &JsonValue) -> Result<AsOf, HostError> {
    let bad = || {
        HostError::InvalidPayload(
            "`as_of` is a time like 2026-03-01T12:00:00Z, or { \"seq\": 3 }".into(),
        )
    };
    match value {
        JsonValue::String(text) => {
            let at = chrono::DateTime::parse_from_rfc3339(text).map_err(|_| bad())?;
            Ok(AsOf::At(at.with_timezone(&chrono::Utc).to_rfc3339()))
        }
        JsonValue::Object(map) => match map.get("seq").and_then(JsonValue::as_u64) {
            Some(seq) if map.len() == 1 && seq >= 1 => Ok(AsOf::Seq(seq)),
            _ => Err(bad()),
        },
        _ => Err(bad()),
    }
}

/// `db::get` with `as_of`: the record as it was then, if the caller may read it as it is today.
pub(super) async fn get_as_of(
    ctx: &PluginHostContext,
    model: &str,
    id: &str,
    as_of: &JsonValue,
) -> Result<JsonValue, HostError> {
    let (grant, _, key) = target(ctx, model, id)?;
    let none = || Ok(json!({ "ok": true, "data": JsonValue::Null }));
    let (seq, from_baseline) = match parse_as_of(as_of)? {
        AsOf::Seq(seq) => (seq, false),
        AsOf::At(at) => {
            let mut binds = record_binds_for(&grant.table, &key);
            binds.push(("at", JsonValue::String(at)));
            let found = rows(
                ctx,
                "SELECT VALUE seq FROM record_versions WHERE model_id = $m AND record_key = $k \
                 AND at <= <datetime> $at ORDER BY seq DESC LIMIT 1;",
                binds,
                0,
            )
            .await?;
            match found.first().and_then(JsonValue::as_u64) {
                Some(seq) => (seq, false),
                // Before the first version: the record is known only if history met it already
                // there (a baseline), and then it was that way until its first change.
                None => {
                    let first = rows(
                        ctx,
                        "SELECT VALUE op FROM record_versions WHERE model_id = $m AND record_key = $k \
                         AND seq = 1;",
                        record_binds_for(&grant.table, &key),
                        0,
                    )
                    .await?;
                    if first.first().and_then(JsonValue::as_str) == Some("baseline") {
                        (1, true)
                    } else {
                        return none();
                    }
                }
            }
        }
    };
    let Some(data) = past_record(ctx, grant, id, &key, seq).await? else {
        return none();
    };
    Ok(json!({ "ok": true, "data": data, "as_of_seq": seq, "from_baseline": from_baseline }))
}

/// The record as it was after version `seq`, as the caller may see it today; `None` when there is
/// no such version, the record did not exist then, or the caller may not read it.
async fn past_record(
    ctx: &PluginHostContext,
    grant: &super::context::ModelGrant,
    id: &str,
    key: &str,
    seq: u64,
) -> Result<Option<JsonValue>, HostError> {
    let versions = load(ctx, &grant.table, key, seq, seq).await?;
    // A version number past the newest is not "the latest": there is no such version.
    if !versions.iter().any(|v| v.seq == seq) {
        return Ok(None);
    }
    let Some(state) = past::state_at(&versions, seq) else {
        return Ok(None);
    };
    let found = visible(ctx, grant, id, vec![(seq, state)]).await?;
    let Some((_, row)) = found.into_iter().next() else {
        return Ok(None);
    };
    Ok(Some(shown(ctx, grant, row).await?))
}

#[derive(Deserialize)]
struct ListRequest {
    model: String,
    id: String,
    #[serde(default)]
    limit: Option<u32>,
    /// Only versions older than this one (to page back).
    #[serde(default)]
    before_seq: Option<u64>,
}

/// `history::list`: the versions of a record, newest first, with who, when and which fields.
pub async fn history_list(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("history::read")?;
    let req: ListRequest = parse_req(payload)?;
    let (grant, schema, key) = target(ctx, &req.model, &req.id)?;
    let empty = || Ok(json!({ "ok": true, "data": [], "next": JsonValue::Null }));

    // The record's story is shown only if the record, in its last live state, is readable today.
    let live = rows(
        ctx,
        "SELECT VALUE seq FROM record_versions WHERE model_id = $m AND record_key = $k \
         AND op != 'delete' ORDER BY seq DESC LIMIT 1;",
        record_binds_for(&grant.table, &key),
        0,
    )
    .await?;
    let Some(live) = live.first().and_then(JsonValue::as_u64) else {
        return empty();
    };
    let versions = load(ctx, &grant.table, &key, live, live).await?;
    let Some(state) = past::state_at(&versions, live) else {
        return empty();
    };
    if visible(ctx, grant, &req.id, vec![(live, state)]).await?.is_empty() {
        return empty();
    }

    let limit = req.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let mut binds = record_binds_for(&grant.table, &key);
    binds.push(("before", json!(req.before_seq.unwrap_or(u64::MAX >> 12))));
    binds.push(("limit", json!(limit + 1)));
    let mut found = rows(
        ctx,
        "SELECT * FROM record_versions WHERE model_id = $m AND record_key = $k AND seq < $before \
         ORDER BY seq DESC LIMIT $limit;",
        binds,
        0,
    )
    .await?;
    let more = found.len() > limit as usize;
    found.truncate(limit as usize);
    let next = if more {
        found.last().and_then(|row| row.get("seq")).cloned()
    } else {
        None
    };
    let mut data = Vec::with_capacity(found.len());
    for row in &found {
        let Some(version) = Version::from_row(row) else {
            continue;
        };
        let fields = names_of(ctx, grant, schema, &past::changed_fields(&version)).await?;
        data.push(json!({
            "seq": version.seq,
            "op": version.op,
            "at": row.get("at"),
            "actor": row.get("actor"),
            "plugin": row.get("plugin"),
            "request_id": row.get("request_id"),
            "fields": fields,
        }));
    }
    Ok(json!({ "ok": true, "data": data, "next": next }))
}

#[derive(Deserialize)]
struct DiffRequest {
    model: String,
    id: String,
    from: u64,
    /// Default: the newest version.
    #[serde(default)]
    to: Option<u64>,
}

/// `history::diff`: field by field, what differs between two versions of a record.
pub async fn history_diff(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("history::read")?;
    let req: DiffRequest = parse_req(payload)?;
    let (grant, _, key) = target(ctx, &req.model, &req.id)?;
    let none = || Ok(json!({ "ok": true, "data": JsonValue::Null }));
    let to = match req.to {
        Some(to) => to,
        None => {
            let newest = rows(
                ctx,
                "SELECT VALUE seq FROM record_versions WHERE model_id = $m AND record_key = $k \
                 ORDER BY seq DESC LIMIT 1;",
                record_binds_for(&grant.table, &key),
                0,
            )
            .await?;
            match newest.first().and_then(JsonValue::as_u64) {
                Some(seq) => seq,
                None => return none(),
            }
        }
    };
    let (lo, hi) = (req.from.min(to), req.from.max(to));
    let versions = load(ctx, &grant.table, &key, lo, hi).await?;
    if ![req.from, to].iter().all(|seq| versions.iter().any(|v| v.seq == *seq)) {
        return none();
    }
    let (Some(before), Some(after)) = (
        past::state_at(&versions, req.from),
        past::state_at(&versions, to),
    ) else {
        return none();
    };
    let found = visible(ctx, grant, &req.id, vec![(req.from, before), (to, after)]).await?;
    let find = |seq: u64| found.iter().find(|(s, _)| *s == seq).map(|(_, row)| row.clone());
    // `from == to` is one state asked twice; both are then the same row.
    let (Some(before), Some(after)) = (find(req.from), find(to)) else {
        return none();
    };
    let (before, after) = (shown(ctx, grant, before).await?, shown(ctx, grant, after).await?);
    let empty = Map::new();
    let changes = past::diff(
        before.as_object().unwrap_or(&empty),
        after.as_object().unwrap_or(&empty),
    );
    let data: Vec<JsonValue> = changes
        .into_iter()
        .map(|change| {
            json!({ "field": change.field, "before": change.before, "after": change.after })
        })
        .collect();
    Ok(json!({ "ok": true, "data": { "from": req.from, "to": to, "changes": data } }))
}

#[derive(Deserialize)]
struct RestoreRequest {
    model: String,
    id: String,
    seq: u64,
}

/// `history::restore`: put a record back to how it was after version `seq`, as one new version.
///
/// It is an ordinary update, so the model's checks, workflow and the caller's record and field
/// rules all apply, and a restore that would break one is refused with that error. Only what
/// differs is written. Fields the caller cannot read are left as they are, as are the fields the
/// kernel calculates or numbers. A deleted record is not brought back.
pub async fn history_restore(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("history::restore")?;
    let req: RestoreRequest = parse_req(payload)?;
    restore_to(ctx, &req.model, &req.id, req.seq).await
}

async fn restore_to(
    ctx: &PluginHostContext,
    model: &str,
    id: &str,
    seq: u64,
) -> Result<JsonValue, HostError> {
    let (grant, schema, key) = target(ctx, model, id)?;
    let Some(old) = past_record(ctx, grant, id, &key, seq).await? else {
        return Err(HostError::InvalidPayload(format!(
            "`{model}` has no version {seq} of this record that you may read"
        )));
    };
    let current = super::db::db_get(ctx, &json!({ "model": model, "id": id })).await?;
    let Some(current) = current.get("data").filter(|data| !data.is_null()).cloned() else {
        return Err(HostError::Denied(
            "the record is gone, or you may not read it; a deleted record cannot be restored".into(),
        ));
    };
    let skip: std::collections::HashSet<&str> = schema
        .derived
        .iter()
        .map(|d| d.field.as_str())
        .chain(schema.sequences.iter().map(|s| s.field.as_str()))
        .chain(["id"])
        .collect();
    // With `fields` set, versions keep only those; the others were never recorded, so they are not
    // "cleared" by a restore.
    let kept: Option<std::collections::HashSet<String>> = match &schema.history {
        Some(keep) => match &keep.field_ids {
            Some(ids) => Some(names_of(ctx, grant, schema, ids).await?.into_iter().collect()),
            None => None,
        },
        None => None,
    };
    let (old, now) = (
        old.as_object().cloned().unwrap_or_default(),
        current.as_object().cloned().unwrap_or_default(),
    );
    let mut names: Vec<&String> = old.keys().chain(now.keys()).collect();
    names.sort();
    names.dedup();
    let mut data = Map::new();
    for name in names {
        if skip.contains(name.as_str()) || kept.as_ref().is_some_and(|k| !k.contains(name)) {
            continue;
        }
        if old.get(name) != now.get(name) {
            data.insert(name.clone(), old.get(name).cloned().unwrap_or(JsonValue::Null));
        }
    }
    if data.is_empty() {
        return Ok(json!({ "ok": true, "data": current, "restored": false, "from_seq": seq }));
    }
    let written = super::db::restore_write(ctx, model.to_string(), id.to_string(), data, seq).await?;
    Ok(json!({ "ok": true, "data": written["data"], "restored": true, "from_seq": seq }))
}

#[derive(Deserialize)]
struct UndoRequest {
    model: String,
    id: String,
}

/// How long after a change its author may undo it, unless the organization sets another.
const DEFAULT_UNDO_WINDOW_SECS: i64 = 120;

/// `history::undo`: take back the caller's own last change to a record, if it is still the last
/// one and recent enough (`history.undo_window_secs`). It is `history::restore` to the version
/// before, so everything `restore` checks applies.
pub async fn history_undo(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("history::restore")?;
    let req: UndoRequest = parse_req(payload)?;
    let (grant, _, key) = target(ctx, &req.model, &req.id)?;
    let newest = rows(
        ctx,
        "SELECT * FROM record_versions WHERE model_id = $m AND record_key = $k \
         ORDER BY seq DESC LIMIT 2;",
        record_binds_for(&grant.table, &key),
        0,
    )
    .await?;
    let refuse = |why: &str| HostError::Denied(format!("nothing to undo: {why}"));
    let last = newest.first().ok_or_else(|| refuse("the record has no history"))?;
    let (seq, op) = (
        last.get("seq").and_then(JsonValue::as_u64).unwrap_or(0),
        last.get("op").and_then(JsonValue::as_str).unwrap_or(""),
    );
    if op != "update" && op != "restore" {
        return Err(refuse("the last version is not a change"));
    }
    let me = ctx.audit.actor.id().unwrap_or("system");
    if last.get("actor").and_then(JsonValue::as_str) != Some(me) {
        return Err(refuse("only your own last change can be undone"));
    }
    let window = super::settings_read::effective_settings(ctx, &["history.undo_window_secs"])
        .await
        .get("history.undo_window_secs")
        .and_then(JsonValue::as_i64)
        .unwrap_or(DEFAULT_UNDO_WINDOW_SECS);
    let at = last
        .get("at")
        .and_then(JsonValue::as_str)
        .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
        .ok_or_else(|| refuse("its time is unknown"))?;
    if chrono::Utc::now().signed_duration_since(at).num_seconds() > window {
        return Err(refuse("it was too long ago"));
    }
    let before = newest
        .get(1)
        .and_then(|row| row.get("seq"))
        .and_then(JsonValue::as_u64)
        .filter(|before| *before + 1 == seq)
        .ok_or_else(|| refuse("there is no earlier version"))?;
    restore_to(ctx, &req.model, &req.id, before).await
}

/// Versions `db::find` with `as_of` reads at most, whatever the organization setting says.
const MAX_FIND_VERSIONS: u32 = 20_000;
/// Records `db::find` with `as_of` searches, unless the organization setting says otherwise.
const DEFAULT_FIND_RECORDS: u64 = 1000;

/// `db::find` with `as_of`: the records that matched `filter` at that time, among those the caller
/// may read today (see the module doc for what that means).
///
/// It rebuilds every record that existed then and filters the old states, so it is bounded: it
/// refuses a model with more than `history.max_as_of_rows` records alive at that time, or whose
/// history up to then is longer than a fixed cap, and says to use a saved report snapshot instead.
/// It takes `filter` and `limit`; `order`, `offset` and `expand` are not available. The rows come
/// back by record key.
pub(super) async fn find_as_of(
    ctx: &PluginHostContext,
    req: &FindRequest,
    as_of: &JsonValue,
) -> Result<JsonValue, HostError> {
    if !req.expand.is_empty() || req.order.is_some() || req.offset.is_some() {
        return Err(HostError::InvalidPayload(
            "`as_of` takes `filter` and `limit`; `order`, `offset` and `expand` are not available with it"
                .into(),
        ));
    }
    let AsOf::At(at) = parse_as_of(as_of)? else {
        return Err(HostError::InvalidPayload(
            "`as_of` for a list is a time like 2026-03-01T12:00:00Z (versions are numbered per record)".into(),
        ));
    };
    let grant = require_model(ctx, &req.model, false)?;
    if !grant.schema.as_deref().is_some_and(|schema| schema.history.is_some()) {
        return Err(HostError::InvalidPayload(format!(
            "`{}` does not keep a history (`history`)",
            req.model
        )));
    }
    let empty = || Ok(json!({ "ok": true, "data": [], "as_of": at }));
    let Some((condition, mut binds, hidden)) =
        guarded_condition(ctx, grant, &req.model, &req.filter).await?
    else {
        return empty();
    };
    let max_records = super::settings_read::effective_settings(ctx, &["history.max_as_of_rows"])
        .await
        .get("history.max_as_of_rows")
        .and_then(JsonValue::as_u64)
        .unwrap_or(DEFAULT_FIND_RECORDS);

    let found = rows(
        ctx,
        "SELECT * FROM record_versions WHERE model_id = $m AND at <= <datetime> $at \
         ORDER BY record_key, seq LIMIT $cap;",
        vec![
            ("m", JsonValue::String(grant.table.clone())),
            ("at", JsonValue::String(at.clone())),
            ("cap", json!(MAX_FIND_VERSIONS + 1)),
        ],
        0,
    )
    .await?;
    let too_much = || {
        HostError::InvalidPayload(
            "too much history before that time to search it as a list; read single records with \
             `as_of`, or use a saved report snapshot"
                .into(),
        )
    };
    if found.len() > MAX_FIND_VERSIONS as usize {
        return Err(too_much());
    }
    // The rows come ordered by record, then version: replay each record's run.
    let mut states: Vec<JsonValue> = Vec::new();
    let mut run: Vec<Version> = Vec::new();
    let mut key = String::new();
    let finish = |key: &str, run: &mut Vec<Version>, states: &mut Vec<JsonValue>| {
        if let Some(last) = run.last().map(|v| v.seq)
            && let Some(mut state) = past::state_at(run, last)
        {
            state.insert("__key".into(), JsonValue::String(key.to_string()));
            states.push(JsonValue::Object(state));
        }
        run.clear();
    };
    for row in &found {
        let (Some(version), Some(record)) = (
            Version::from_row(row),
            row.get("record_key").and_then(JsonValue::as_str),
        ) else {
            continue;
        };
        if record != key {
            finish(&key, &mut run, &mut states);
            key = record.to_string();
        }
        run.push(version);
    }
    finish(&key, &mut run, &mut states);
    if states.len() as u64 > max_records {
        return Err(too_much());
    }
    if states.is_empty() {
        return empty();
    }

    let limit = req
        .limit
        .map_or(super::db::MAX_ROWS_PER_FIND, |l| l.min(super::db::MAX_ROWS_PER_FIND));
    let filter = if condition.is_empty() {
        String::new()
    } else {
        format!(" WHERE {condition}")
    };
    // `id` is made a record first so the rules and the filter can use it like on a stored row.
    let body = format!(
        "LET $rows = (SELECT * FROM (SELECT *, type::record($__table, __key) AS id \
         FROM $__states){filter} LIMIT {limit});"
    );
    binds.push(("__table".to_string(), JsonValue::String(grant.table.clone())));
    binds.push(("__states".to_string(), JsonValue::Array(states)));
    let rows = run_audited(
        ctx,
        Access {
            operation: "read",
            model: req.model.clone(),
            table: &grant.table,
        },
        &body,
        "$rows.id",
        binds,
        Extra::default(),
    )
    .await?;
    let data: Vec<JsonValue> = rows
        .into_iter()
        .map(|mut row| {
            if let Some(map) = row.as_object_mut() {
                map.remove("__key");
            }
            guard::strip(&hidden, decode(grant, row))
        })
        .collect();
    Ok(json!({ "ok": true, "data": data, "as_of": at }))
}

#[derive(Deserialize)]
struct RedactRequest {
    model: String,
    id: String,
    fields: Vec<String>,
    #[serde(default)]
    reason: Option<String>,
}

/// What stands where a redacted value was.
pub const REDACTED: &str = "[redacted]";

/// `history::redact`: blot out the values of some fields in every version of a record, and in the
/// chatter lines that tracked them, for a legal erasure. The versions stay (who and when) and the
/// change is logged in `history_redactions`. Only an administrator may do it; it does not touch the
/// record itself (change that with an update first), and it cannot be undone.
pub async fn history_redact(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("history::redact")?;
    let req: RedactRequest = parse_req(payload)?;
    let (grant, schema, _) = target(ctx, &req.model, &req.id)?;
    if !guard::exempt(ctx).await? {
        return Err(HostError::Denied(
            "only an administrator may redact a record's history".into(),
        ));
    }
    if req.fields.is_empty() || req.fields.len() > 50 {
        return Err(HostError::InvalidPayload(
            "name 1 to 50 fields to redact".into(),
        ));
    }
    let mut ids = Vec::new();
    for name in &req.fields {
        let id = schema
            .column_id(name)
            .map_err(|e| HostError::InvalidPayload(e.to_string()))?;
        super::db::validate_ident(id)?;
        ids.push(id.to_string());
    }
    let mut extra = Extra::default();
    for id in &ids {
        for (table, columns) in [
            ("record_versions", ["snapshot", "delta"]),
            ("chatter_messages", ["before", "after"]),
        ] {
            for column in columns {
                extra.before.push(format!(
                    "UPDATE {table} SET {column}.{id} = '{REDACTED}' WHERE model_id = $__table \
                     AND record_key = $__key AND {column}.{id} != NONE;"
                ));
            }
        }
    }
    extra.before.push(
        "CREATE history_redactions SET model_id = $__table, record_key = $__key, \
         fields = $__fields, reason = $__reason, actor = $__cm_author;"
            .to_string(),
    );
    let mut binds = record_binds(grant, &req.id);
    binds.push(("__fields".to_string(), json!(ids)));
    binds.push(("__reason".to_string(), json!(req.reason)));
    run_audited(
        ctx,
        Access {
            operation: "redact",
            model: req.model.clone(),
            table: &grant.table,
        },
        "LET $rows = (SELECT * FROM type::record($__table, $__key));",
        RECORD_ID,
        binds,
        extra,
    )
    .await?;
    Ok(json!({ "ok": true, "data": { "redacted": req.fields } }))
}
