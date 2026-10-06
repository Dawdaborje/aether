//! A record and its child rows, written and read together.
//!
//! A model with a `child` field (`"lines": { "type": "child", "target": "invoice_line", "inverse": "invoice" }`)
//! takes its rows in the same call as the record:
//!
//! * `db::create` with `"lines": [ {…}, {…} ]` writes the record and its rows in one transaction.
//! * `db::update` with `"lines": [ … ]` makes the rows exactly that list: a row with an `id` is
//!   changed, a row without one is added, and a row of the record that is not in the list is
//!   deleted. Only rows the caller may read take part, so rules that hide rows are never undone
//!   by an omission. A field left out of the update is not touched.
//! * `db::delete` deletes the rows with the record, and is refused when the record has rows the
//!   caller cannot see.
//! * `db::get` and `db::find` with `"expand": ["lines"]` return the rows too, in order.
//!
//! Every row is an ordinary record of its own model: it is checked, ruled and audited like any
//! other write, and totals over the rows (`sum(lines.amount)`) are worked out after the last row.

use std::collections::{BTreeSet, HashMap};

use serde_json::{Map, Value as JsonValue, json};

use super::context::PluginHostContext;
use super::db::{
    CreateRequest, DeleteRequest, Injection, Options, Plan, UpdateRequest, plan_create_with, plan_delete_with, plan_update_with,
    require_model, run_composed, strip_table_prefix,
};
use super::error::HostError;
use super::integrity;
use crate::data_model::ModelSchema;
use crate::data_model::runtime::ChildField;

/// Most rows one call writes (or reads for one expansion).
pub const MAX_ROWS: usize = 200;
/// Rows of one child field one record may have for them to be listed, replaced or deleted together.
const MAX_EXISTING: usize = 1000;
/// Records one `expand` handles in a query.
const EXPAND_CHUNK: usize = 400;

fn invalid(message: impl Into<String>) -> HostError {
    HostError::InvalidPayload(message.into())
}

type Rows<'a> = Vec<(&'a ChildField, Vec<Map<String, JsonValue>>)>;

/// The record's own values, and the rows of each child field it names.
fn split<'a>(schema: &'a ModelSchema, data: &Map<String, JsonValue>) -> Result<(Map<String, JsonValue>, Rows<'a>), HostError> {
    let mut own = Map::new();
    let mut groups: Rows<'a> = Vec::new();
    let mut total = 0;
    for (name, value) in data {
        let Some(child) = schema.child(name) else {
            own.insert(name.clone(), value.clone());
            continue;
        };
        let list = value.as_array().ok_or_else(|| invalid(format!("`{name}` is a list of rows")))?;
        let mut rows = Vec::with_capacity(list.len());
        for (position, row) in list.iter().enumerate() {
            let row = row.as_object().ok_or_else(|| invalid(format!("{name}[{position}] must be an object")))?;
            rows.push(row.clone());
        }
        total += rows.len();
        groups.push((child, rows));
    }
    if total > MAX_ROWS {
        return Err(invalid(format!("at most {MAX_ROWS} rows are written in one call, not {total}")));
    }
    Ok((own, groups))
}

/// A row as it is written: numbered by its place when the field says so.
fn numbered(child: &ChildField, mut row: Map<String, JsonValue>, position: usize) -> Map<String, JsonValue> {
    if let Some((field, _)) = &child.order {
        row.insert(field.clone(), JsonValue::from(position as i64 + 1));
    }
    row
}

fn at(child: &ChildField, position: usize, error: HostError) -> HostError {
    match error {
        HostError::InvalidPayload(message) => HostError::InvalidPayload(format!("{}[{position}]: {message}", child.field)),
        other => other,
    }
}

/// After the last row: work out the record's totals and checks again, now that its rows are final.
fn finish(schema: &ModelSchema, plans: &mut [Plan<'_>]) -> Result<(), HostError> {
    if plans.len() < 2 {
        return Ok(());
    }
    if let Some(last) = plans.last_mut() {
        last.extra.after.extend(integrity::derived_again(schema, "$rows_0")?);
        integrity::model_checks(schema, "$rows_0", &mut last.binds, &mut last.extra)?;
    }
    Ok(())
}

/// The record with its rows put in.
fn assemble(mut record: JsonValue, listed: Vec<(&ChildField, Vec<JsonValue>)>) -> JsonValue {
    if let Some(object) = record.as_object_mut() {
        for (child, rows) in listed {
            object.insert(child.field.clone(), JsonValue::Array(rows));
        }
    }
    record
}

/// A create that carries rows; `None` when it carries none.
pub(super) async fn create(ctx: &PluginHostContext, req: &CreateRequest) -> Result<Option<JsonValue>, HostError> {
    let grant = require_model(ctx, &req.model, true)?;
    let Some(schema) = &grant.schema else { return Ok(None) };
    if schema.children.is_empty() || !req.data.keys().any(|key| schema.child(key).is_some()) {
        return Ok(None);
    }
    let (own, groups) = split(schema, &req.data)?;
    let parent = Options { allow_empty: true, defer_checks: true, ..Options::default() };
    let mut plans = vec![plan_create_with(ctx, req.model.clone(), own, parent).await?];
    let mut listed = Vec::new();
    for (child, rows) in groups {
        let mut indexes = Vec::new();
        for (position, row) in rows.into_iter().enumerate() {
            if row.contains_key(&child.inverse_field) || row.contains_key("id") {
                return Err(invalid(format!("{}[{position}]: a row has no `id` or `{}` here, the kernel sets them", child.field, child.inverse_field)));
            }
            let options = Options {
                inject: Some(Injection {
                    field: child.inverse_field.clone(),
                    column: child.inverse_column.clone(),
                    value_sql: "$rows_0[0].id".into(),
                }),
                allow_empty: true,
                skip_parent_refresh: true,
                ..Options::default()
            };
            let plan = plan_create_with(ctx, child.model.clone(), numbered(child, row, position), options)
                .await
                .map_err(|error| at(child, position, error))?;
            indexes.push(plans.len());
            plans.push(plan);
        }
        listed.push((child, indexes));
    }
    finish(schema, &mut plans)?;
    let results = run_composed(ctx, &plans).await?;
    let record = results.first().cloned().unwrap_or(JsonValue::Null);
    let listed = listed
        .into_iter()
        .map(|(child, indexes)| (child, indexes.into_iter().filter_map(|i| results.get(i).cloned()).collect()))
        .collect();
    Ok(Some(json!({ "ok": true, "data": assemble(record, listed) })))
}

/// The ids of the rows of `record` (by `child`) the caller can read, as record keys.
async fn visible_rows(ctx: &PluginHostContext, child: &ChildField, record: &str) -> Result<Vec<String>, HostError> {
    let reply = super::db::find_records(
        ctx,
        &json!({ "model": child.model, "filter": { child.inverse_field.clone(): record }, "limit": MAX_EXISTING }),
    )
    .await?;
    let rows = reply.get("data").and_then(JsonValue::as_array).cloned().unwrap_or_default();
    if rows.len() >= MAX_EXISTING {
        return Err(invalid(format!("`{}` has too many rows to replace together", child.field)));
    }
    Ok(rows.iter().filter_map(|row| row.get("id").and_then(JsonValue::as_str)).map(|id| strip_table_prefix(id, &child.table).to_string()).collect())
}

/// An update that carries rows; `None` when it carries none.
pub(super) async fn update(ctx: &PluginHostContext, req: &UpdateRequest) -> Result<Option<JsonValue>, HostError> {
    let grant = require_model(ctx, &req.model, true)?;
    let Some(schema) = &grant.schema else { return Ok(None) };
    if schema.children.is_empty() || !req.data.keys().any(|key| schema.child(key).is_some()) {
        return Ok(None);
    }
    let (own, groups) = split(schema, &req.data)?;
    let parent = Options { allow_empty: true, defer_checks: true, ..Options::default() };
    let mut plans = vec![plan_update_with(ctx, req.model.clone(), req.id.clone(), own, parent).await?];
    let mut listed = Vec::new();
    for (child, rows) in groups {
        let existing: BTreeSet<String> = visible_rows(ctx, child, &req.id).await?.into_iter().collect();
        let mut kept = BTreeSet::new();
        let mut indexes = Vec::new();
        for (position, mut row) in rows.into_iter().enumerate() {
            if let Some(back) = row.remove(&child.inverse_field)
                && back.as_str().map(|id| strip_table_prefix(id, &grant.table)) != Some(strip_table_prefix(&req.id, &grant.table))
            {
                return Err(invalid(format!("{}[{position}]: `{}` is this record", child.field, child.inverse_field)));
            }
            let id = match row.remove("id") {
                None | Some(JsonValue::Null) => None,
                Some(JsonValue::String(id)) => Some(strip_table_prefix(&id, &child.table).to_string()),
                Some(_) => return Err(invalid(format!("{}[{position}]: `id` is text", child.field))),
            };
            let options = Options { allow_empty: true, skip_parent_refresh: true, ..Options::default() };
            let plan = match id {
                Some(id) => {
                    if !existing.contains(&id) {
                        return Err(invalid(format!("{}[{position}]: `{id}` is not a row of this record", child.field)));
                    }
                    if !kept.insert(id.clone()) {
                        return Err(invalid(format!("{}[{position}]: `{id}` is listed twice", child.field)));
                    }
                    plan_update_with(ctx, child.model.clone(), id, numbered(child, row, position), options).await
                }
                None => {
                    let mut row = numbered(child, row, position);
                    row.insert(child.inverse_field.clone(), JsonValue::String(req.id.clone()));
                    plan_create_with(ctx, child.model.clone(), row, options).await
                }
            }
            .map_err(|error| at(child, position, error))?;
            indexes.push(plans.len());
            plans.push(plan);
        }
        for id in existing.difference(&kept) {
            let options = Options { skip_parent_refresh: true, ..Options::default() };
            plans.push(plan_delete_with(ctx, child.model.clone(), id.clone(), options).await?);
        }
        listed.push((child, indexes));
    }
    finish(schema, &mut plans)?;
    let results = run_composed(ctx, &plans).await?;
    let record = results.first().cloned().unwrap_or(JsonValue::Null);
    let listed = listed
        .into_iter()
        .map(|(child, indexes)| (child, indexes.into_iter().filter_map(|i| results.get(i).cloned()).collect()))
        .collect();
    Ok(Some(json!({ "ok": true, "data": assemble(record, listed) })))
}

/// A delete of a record that has rows; `None` when it has none.
pub(super) async fn delete(ctx: &PluginHostContext, req: &DeleteRequest) -> Result<Option<JsonValue>, HostError> {
    let grant = require_model(ctx, &req.model, true)?;
    let Some(schema) = &grant.schema else { return Ok(None) };
    if schema.children.is_empty() {
        return Ok(None);
    }
    let mut plans = Vec::new();
    for child in &schema.children {
        let visible = visible_rows(ctx, child, &req.id).await?;
        let mut response = ctx
            .db
            .query(format!("SELECT count() AS n FROM {} WHERE {} = $record GROUP ALL;", child.table, child.inverse_column))
            .bind(("record", format!("{}:{}", grant.table, strip_table_prefix(&req.id, &grant.table))))
            .await?;
        let counted: Vec<JsonValue> = response.take(0)?;
        let total = counted.first().and_then(|row| row.get("n")).and_then(JsonValue::as_u64).unwrap_or(0) as usize;
        if total > visible.len() {
            return Err(invalid(format!("`{}` has rows you may not see, so the record cannot be deleted", child.field)));
        }
        for id in visible {
            let options = Options { skip_parent_refresh: true, ..Options::default() };
            plans.push(plan_delete_with(ctx, child.model.clone(), id, options).await?);
        }
    }
    if plans.is_empty() {
        return Ok(None);
    }
    plans.push(plan_delete_with(ctx, req.model.clone(), req.id.clone(), Options::default()).await?);
    let mut results = run_composed(ctx, &plans).await?;
    Ok(Some(json!({ "ok": true, "data": results.pop().unwrap_or(JsonValue::Null) })))
}

/// Put the rows of the named child fields into already-read records.
pub(super) async fn expand(
    ctx: &PluginHostContext,
    schema: &ModelSchema,
    records: &mut [JsonValue],
    names: &[String],
) -> Result<(), HostError> {
    for name in names {
        let child = schema.child(name).ok_or_else(|| invalid(format!("`{name}` is not a child field of `{}`", schema.name)))?;
        let ids: Vec<String> =
            records.iter().filter_map(|record| record.get("id").and_then(JsonValue::as_str)).map(str::to_string).collect();
        let mut by_record: HashMap<String, Vec<JsonValue>> = HashMap::new();
        for chunk in ids.chunks(EXPAND_CHUNK) {
            let mut payload = json!({
                "model": child.model,
                "filter": { child.inverse_field.clone(): { "in": chunk } },
                "limit": MAX_EXISTING,
            });
            if let Some((order, _)) = &child.order {
                payload["order"] = JsonValue::String(order.clone());
            }
            let reply = super::db::find_records(ctx, &payload).await?;
            let rows = reply.get("data").and_then(JsonValue::as_array).cloned().unwrap_or_default();
            if rows.len() >= MAX_EXISTING {
                return Err(invalid(format!("`{name}` has too many rows to expand for this many records")));
            }
            for row in rows {
                if let Some(owner) = row.get(&child.inverse_field).and_then(JsonValue::as_str) {
                    by_record.entry(owner.to_string()).or_default().push(row);
                }
            }
        }
        for record in records.iter_mut() {
            let rows = record.get("id").and_then(JsonValue::as_str).and_then(|id| by_record.remove(id)).unwrap_or_default();
            if let Some(object) = record.as_object_mut() {
                object.insert(name.clone(), JsonValue::Array(rows));
            }
        }
    }
    Ok(())
}
