//! `payroll::calculate` and `payroll::write`: payslips with the Swift payroll engine, natively and on every core,
//! written without their lines ever passing through the plugin's WebAssembly.
//!
//! ```json
//! { "structures": { "<key>": <definition> | null },
//!   "structure_records": { "<key>": { "model": "pay_structure", "id": "…", "field": "definition" } },
//!   "people": [ { "id": "employees:ann", "structure": "<key>", "variables": { "base_salary": 4200 } } ],
//!   "lines": "text" | "keep" }
//! ```
//!
//! The key is the plugin's name for a structure and must change whenever the structure does (its hash). A
//! structure the kernel already holds under that key may be sent as `null`. One it does not hold is compiled from
//! its definition: sent in `structures`, or read by the kernel from the record named in `structure_records` (with
//! the plugin's own grants), so a large definition need not be read into the plugin at all. If neither is given
//! the answer is `{ "missing": [keys] }` and nothing is calculated. Otherwise the answer is what
//! [`aether_payroll::calculate`] gives: one result per person and the sum of every component.
//!
//! `"lines": "text"`: each payslip's amounts come as one comma-joined string. `"lines": "keep"`: they stay in the
//! kernel and the answer has a `ticket` instead; `payroll::write` then puts them into the payslips it writes. The
//! work does not count against the plugin's fuel. Needs the capability `payroll::calculate`.
//!
//! ```json
//! { "ticket": "…", "lines_field": "lines", "ops": [ <db::transaction ops>, … ] }
//! ```
//!
//! `payroll::write` runs `ops` as one quiet `db::transaction` (every write checked as when sent alone); each
//! `create` op with `"lines_of": "<person id>"` gets that person's kept lines in `data[lines_field]`. A ticket is
//! used once; if the transaction fails the lines are kept again, so the same write may be retried.

use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use aether_payroll::{CalcError, Person};
use serde::Deserialize;
use serde_json::{Map, Value as JsonValue, json};

use super::context::PluginHostContext;
use super::error::HostError;

#[derive(Deserialize)]
struct Request {
    #[serde(default)]
    structures: Map<String, JsonValue>,
    #[serde(default)]
    structure_records: BTreeMap<String, RecordRef>,
    people: Vec<PersonRequest>,
    /// `"text"`: each payslip's amounts as one comma-joined string instead of an array. `"keep"`: kept here.
    #[serde(default)]
    lines: Option<String>,
}

#[derive(Deserialize)]
struct RecordRef {
    model: String,
    id: String,
    #[serde(default = "definition_field")]
    field: String,
}

fn definition_field() -> String {
    "definition".into()
}

#[derive(Deserialize)]
struct PersonRequest {
    id: String,
    structure: String,
    #[serde(default)]
    variables: BTreeMap<String, JsonValue>,
}

fn scope_of(ctx: &PluginHostContext) -> String {
    format!("{}/{}", ctx.database, ctx.plugin_name)
}

pub async fn calculate(ctx: &PluginHostContext, payload: JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("payroll::calculate")?;
    let mut request: Request = serde_json::from_value(payload).map_err(|error| HostError::InvalidPayload(error.to_string()))?;
    let scope = scope_of(ctx);
    let cache = aether_payroll::cache();
    // Definitions the kernel reads itself: only those it does not hold already, and only for keys in use.
    for (key, record) in &request.structure_records {
        let given = request.structures.get(key).is_some_and(|d| !d.is_null());
        if given || cache.holds(&scope, key) {
            continue;
        }
        let reply = super::db::db_get(ctx, &json!({ "model": record.model, "id": record.id })).await?;
        let definition = reply.get("data").and_then(|row| row.get(&record.field)).cloned().unwrap_or(JsonValue::Null);
        if definition.is_null() {
            return Err(HostError::InvalidPayload(format!(
                "structure {key}: {} {} has no `{}` the plugin may read",
                record.model, record.id, record.field
            )));
        }
        request.structures.insert(key.clone(), definition);
    }
    let people: Vec<Person> = request
        .people
        .into_iter()
        .map(|p| Person { id: p.id, structure: p.structure, variables: p.variables })
        .collect();
    let keep = request.lines.as_deref() == Some("keep");
    let text = keep || request.lines.as_deref() == Some("text");
    let started = Instant::now();
    // The plugin's call already runs on a thread of its own (a host function blocks it), so the work is
    // done here; rayon's threads take the people when there are enough of them.
    match aether_payroll::calculate(cache, &scope, &request.structures, &people, text) {
        Ok(mut data) => {
            if keep {
                let mut kept = HashMap::new();
                for result in data["results"].as_array_mut().into_iter().flatten() {
                    let id = result["id"].as_str().map(str::to_string);
                    let lines = result.as_object_mut().and_then(|r| r.remove("lines"));
                    if let (Some(id), Some(JsonValue::String(lines))) = (id, lines) {
                        kept.insert(id, lines);
                    }
                }
                data["ticket"] = json!(aether_payroll::tickets().keep(&scope, kept));
            }
            data["micros"] = json!(started.elapsed().as_micros() as u64);
            Ok(json!({ "ok": true, "data": data }))
        }
        Err(CalcError::Missing(keys)) => Ok(json!({ "ok": true, "data": { "missing": keys } })),
        Err(CalcError::Invalid(message)) => Err(HostError::InvalidPayload(message)),
    }
}

#[derive(Deserialize)]
struct WriteRequest {
    ticket: String,
    #[serde(default = "lines_field")]
    lines_field: String,
    ops: Vec<JsonValue>,
}

fn lines_field() -> String {
    "lines".into()
}

/// Put each `lines_of` op's kept lines into its data. On a refusal nothing is changed and the lines stay kept.
fn fill_lines(ops: &mut [JsonValue], lines: &HashMap<String, String>, field: &str) -> Result<(), HostError> {
    for op in ops.iter() {
        let Some(person) = op.get("lines_of") else { continue };
        let person = person.as_str().unwrap_or_default();
        if !lines.contains_key(person) {
            return Err(HostError::InvalidPayload(format!("the ticket holds no lines for {person}")));
        }
        if !op.get("data").is_some_and(JsonValue::is_object) || op.get("op").and_then(JsonValue::as_str) != Some("create") {
            return Err(HostError::InvalidPayload("a `lines_of` op is a `create` with `data`".into()));
        }
    }
    for op in ops.iter_mut() {
        let Some(object) = op.as_object_mut() else { continue };
        let Some(person) = object.remove("lines_of") else { continue };
        let text = lines.get(person.as_str().unwrap_or_default()).cloned().unwrap_or_default();
        if let Some(data) = object.get_mut("data").and_then(JsonValue::as_object_mut) {
            data.insert(field.to_string(), JsonValue::String(text));
        }
    }
    Ok(())
}

pub async fn write(ctx: &PluginHostContext, payload: JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("payroll::calculate")?;
    let mut request: WriteRequest = serde_json::from_value(payload).map_err(|error| HostError::InvalidPayload(error.to_string()))?;
    let scope = scope_of(ctx);
    let tickets = aether_payroll::tickets();
    let lines = tickets
        .take(&scope, &request.ticket)
        .ok_or_else(|| HostError::InvalidPayload("the ticket is unknown or expired: calculate again".into()))?;
    let outcome = match fill_lines(&mut request.ops, &lines, &request.lines_field) {
        Ok(()) => super::db::db_transaction(ctx, &json!({ "ops": request.ops, "quiet": true })).await,
        Err(error) => Err(error),
    };
    if outcome.is_err() {
        tickets.put_back(&scope, &request.ticket, lines);
    }
    outcome
}

#[derive(Deserialize)]
struct SumRequest {
    model: String,
    #[serde(default)]
    filter: Map<String, JsonValue>,
    #[serde(default)]
    order: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
    /// The object field holding `{ key: lines text }` (a batch's `totals`) and the key under it (`sums`).
    field: String,
    key: String,
    /// Running sums to add these to, by the same keys.
    #[serde(default)]
    add_to: BTreeMap<String, String>,
}

/// `payroll::sum_lines`: add up comma-joined amounts kept in records, natively, without the plugin reading them.
///
/// ```json
/// { "model": "pay_batch", "filter": { … }, "order": "idx", "limit": 200, "field": "totals", "key": "sums",
///   "add_to": { "<structure>": "1.00,2.00" } }
/// ```
///
/// Finds the records as `db::query` does (with the plugin's grants), and for each adds every `field.key.<name>`
/// text to `add_to[name]`, position by position. Answers `{ "sums": { name: text }, "rows": n }`. A pay run's
/// finalising uses it: a structure of thousands of components has thousands of sums in every batch report.
pub async fn sum_lines(ctx: &PluginHostContext, payload: JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("payroll::calculate")?;
    let request: SumRequest = serde_json::from_value(payload).map_err(|error| HostError::InvalidPayload(error.to_string()))?;
    let mut find = json!({ "model": request.model, "filter": request.filter });
    if let Some(order) = &request.order {
        find["order"] = json!(order);
    }
    if let Some(limit) = request.limit {
        find["limit"] = json!(limit);
    }
    let reply = super::db::db_find(ctx, &find).await?;
    let rows = reply.get("data").and_then(JsonValue::as_array).cloned().unwrap_or_default();
    let mut totals: BTreeMap<String, Vec<aether_payroll::Decimal>> = BTreeMap::new();
    for (name, text) in &request.add_to {
        aether_payroll::add_lines(totals.entry(name.clone()).or_default(), text).map_err(HostError::InvalidPayload)?;
    }
    for row in &rows {
        for (name, text) in row.get(&request.field).and_then(|f| f.get(&request.key)).and_then(JsonValue::as_object).into_iter().flatten() {
            let text = text.as_str().unwrap_or_default();
            aether_payroll::add_lines(totals.entry(name.clone()).or_default(), text).map_err(HostError::InvalidPayload)?;
        }
    }
    let sums: Map<String, JsonValue> = totals.into_iter().map(|(name, total)| (name, JsonValue::String(aether_payroll::lines_text(&total)))).collect();
    Ok(json!({ "ok": true, "data": { "sums": sums, "rows": rows.len() } }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_go_into_the_creates_that_name_their_person() {
        let lines = HashMap::from([("e:1".to_string(), "1.00,2.00".to_string())]);
        let mut ops = vec![
            json!({ "op": "create", "model": "payslip", "data": { "net": 1 }, "lines_of": "e:1" }),
            json!({ "op": "create", "model": "pay_batch", "data": { "idx": 0 } }),
        ];
        fill_lines(&mut ops, &lines, "lines").unwrap();
        assert_eq!(ops[0], json!({ "op": "create", "model": "payslip", "data": { "net": 1, "lines": "1.00,2.00" } }));
        assert_eq!(ops[1]["data"], json!({ "idx": 0 }));
        let mut unknown = vec![json!({ "op": "create", "model": "payslip", "data": {}, "lines_of": "e:2" })];
        assert!(fill_lines(&mut unknown, &lines, "lines").is_err());
        assert!(unknown[0].get("lines_of").is_some(), "a refused write is left as it was");
        let mut update = vec![json!({ "op": "update", "model": "payslip", "id": "x", "data": {}, "lines_of": "e:1" })];
        assert!(fill_lines(&mut update, &lines, "lines").is_err());
    }
}
