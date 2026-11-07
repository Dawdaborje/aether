//! Applying rows to a model: for each row of the file, decide whether it creates a record, changes one,
//! changes nothing, or cannot be used, and do it through the same `db::*` commands a plugin uses, as the
//! person importing, so the model's checks, record and field rules, workflow and naming series all apply.
//! In a dry run the context takes every write back (see `PluginHostContext::dry_run`).

use std::collections::HashMap;

use serde::Serialize;
use serde_json::{Map, Value, json};

use super::mapping::{CellProblem, Mapping, coerce, importable};
use super::table::Row;
use crate::data_model::{FieldDef, FieldType, ModelDef};
use crate::kernel::{HostError, PluginHostContext, kernel_command};

/// What a row came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Create,
    Update,
    Unchanged,
    Error,
}

impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Update => "update",
            Self::Unchanged => "unchanged",
            Self::Error => "error",
        }
    }
}

/// The result for one row of the file.
#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    /// The row's number in the file (the headers are row 1).
    pub row: usize,
    pub action: Action,
    /// The record created or changed (not known for a created record in a dry run).
    pub record_id: Option<String>,
    pub message: Option<String>,
    pub problems: Vec<CellProblem>,
    /// For an update: the old values of the fields that changed, and what they became (to take it back).
    pub before: Option<Map<String, Value>>,
    pub after: Option<Map<String, Value>>,
}

impl Outcome {
    fn error(row: usize, message: impl Into<String>, problems: Vec<CellProblem>) -> Self {
        Self { row, action: Action::Error, record_id: None, message: Some(message.into()), problems, before: None, after: None }
    }
}

/// The mapping applied to a file's headers: which column is which field, checked against the model.
#[derive(Debug)]
pub struct Resolved<'a> {
    pub model: &'a ModelDef,
    /// For each column of the file, in order: the field it goes to.
    pub columns: Vec<Option<&'a FieldDef>>,
    pub headers: Vec<String>,
    pub mapping: &'a Mapping,
}

/// Check a mapping against the model and the file's headers before any row is read. A problem here
/// would fail every row the same way, so it fails the import instead.
pub fn resolve<'a>(model: &'a ModelDef, headers: &[String], mapping: &'a Mapping) -> Result<Resolved<'a>, String> {
    let field_of = |name: &str| model.fields.iter().find(|f| f.name == name);
    for (header, field) in &mapping.columns {
        if let Some(name) = field {
            match field_of(name) {
                None => return Err(format!("the column `{header}` is mapped to `{name}`, which is not a field of `{}`", model.name)),
                Some(f) if !importable(f) => return Err(format!("the column `{header}` is mapped to `{name}`, which cannot be given a value from a file")),
                Some(_) => {}
            }
        }
    }
    let columns: Vec<Option<&FieldDef>> = headers
        .iter()
        .map(|header| mapping.columns.get(header).and_then(|name| name.as_deref()).and_then(field_of))
        .collect();
    let mapped = |name: &str| columns.iter().flatten().any(|f| f.name == name);
    let mut twice: Vec<&str> = Vec::new();
    for field in columns.iter().flatten() {
        if columns.iter().flatten().filter(|f| f.name == field.name).count() > 1 && !twice.contains(&field.name.as_str()) {
            twice.push(&field.name);
        }
    }
    if let Some(name) = twice.first() {
        return Err(format!("two columns are mapped to `{name}`"));
    }
    for name in &mapping.key {
        if field_of(name).is_none() {
            return Err(format!("the key `{name}` is not a field of `{}`", model.name));
        }
        if !mapped(name) {
            return Err(format!("the key `{name}` has no column in the file"));
        }
    }
    for name in mapping.defaults.keys() {
        if field_of(name).is_none() {
            return Err(format!("the default for `{name}`: it is not a field of `{}`", model.name));
        }
    }
    for (field, target) in &mapping.link_by {
        match field_of(field) {
            Some(f) if f.kind == FieldType::Link => {}
            _ => return Err(format!("`link_by` names `{field}`, which is not a link field of `{}`", model.name)),
        }
        if target.trim().is_empty() {
            return Err(format!("`link_by` for `{field}` names no field to look records up by"));
        }
    }
    if columns.iter().flatten().next().is_none() {
        return Err("no column is mapped to a field".into());
    }
    for f in &model.fields {
        if f.required && f.default.is_none() && f.sequence.is_none() && importable(f) && !mapped(&f.name) && !mapping.defaults.contains_key(&f.name) {
            return Err(format!("`{}` is required, and no column or default gives it a value", f.name));
        }
    }
    Ok(Resolved { model, columns, headers: headers.to_vec(), mapping })
}

/// Two values are the same for an update when they are equal, numbers by value.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}

/// Lookups of linked records by a field other than the id, remembered for the run.
#[derive(Default)]
pub struct LinkCache(HashMap<(String, String), Result<String, String>>);

async fn find_linked(
    ctx: &PluginHostContext,
    field: &FieldDef,
    by: &str,
    text: &str,
    cache: &mut LinkCache,
) -> Result<String, String> {
    let key = (field.name.clone(), text.to_string());
    if let Some(known) = cache.0.get(&key) {
        return known.clone();
    }
    let target = field.target.clone().ok_or_else(|| format!("`{}` has no target model to look up", field.name))?;
    let found = kernel_command(ctx, "db::find", json!({ "model": target, "filter": { by: text }, "limit": 2 }))
        .await
        .map_err(|e| e.to_string())
        .and_then(|reply| {
            let rows = reply.get("data").and_then(Value::as_array).cloned().unwrap_or_default();
            match rows.as_slice() {
                [one] => one.get("id").and_then(Value::as_str).map(str::to_string).ok_or_else(|| "the linked record has no id".to_string()),
                [] => Err(format!("no `{target}` has {by} = {text}")),
                _ => Err(format!("more than one `{target}` has {by} = {text}")),
            }
        });
    cache.0.insert(key, found.clone());
    found
}

/// Decide and do one row.
pub async fn apply_row(ctx: &PluginHostContext, resolved: &Resolved<'_>, row: &Row, links: &mut LinkCache) -> Outcome {
    let (model, mapping) = (resolved.model, resolved.mapping);
    let mut data = Map::new();
    let mut problems = Vec::new();
    for (index, field) in resolved.columns.iter().enumerate() {
        let Some(field) = field else { continue };
        let text = row.cells.get(index).map_or("", String::as_str);
        let header = resolved.headers.get(index).cloned().unwrap_or_default();
        if let Some(by) = mapping.link_by.get(&field.name) {
            if text.trim().is_empty() {
                continue;
            }
            match find_linked(ctx, field, by, text.trim(), links).await {
                Ok(id) => {
                    data.insert(field.name.clone(), Value::String(id));
                }
                Err(message) => problems.push(CellProblem { column: header, value: text.to_string(), message }),
            }
            continue;
        }
        match coerce(field, text, &mapping.notation, mapping.values.get(&field.name)) {
            Ok(Some(value)) => {
                data.insert(field.name.clone(), value);
            }
            Ok(None) => {}
            Err(message) => problems.push(CellProblem { column: header, value: text.to_string(), message }),
        }
    }
    if !problems.is_empty() {
        let first = &problems[0];
        let message = format!("`{}` {}", first.column, first.message);
        return Outcome::error(row.number, message, problems);
    }

    // Is there a record this row is about?
    let existing = if mapping.key.is_empty() {
        None
    } else {
        let mut filter = Map::new();
        for name in &mapping.key {
            match data.get(name) {
                Some(value) => {
                    filter.insert(name.clone(), value.clone());
                }
                None => return Outcome::error(row.number, format!("`{name}` is empty, and it identifies the record"), vec![]),
            }
        }
        match kernel_command(ctx, "db::find", json!({ "model": model.name, "filter": filter, "limit": 2 })).await {
            Err(error) => return Outcome::error(row.number, error.to_string(), vec![]),
            Ok(reply) => {
                let rows = reply.get("data").and_then(Value::as_array).cloned().unwrap_or_default();
                match rows.len() {
                    0 => None,
                    1 => rows.into_iter().next(),
                    _ => return Outcome::error(row.number, format!("more than one record has the same {}", mapping.key.join(", ")), vec![]),
                }
            }
        }
    };

    match existing {
        None => {
            let mut full = data.clone();
            for (name, value) in &mapping.defaults {
                full.entry(name.clone()).or_insert_with(|| value.clone());
            }
            if full.is_empty() {
                return Outcome::error(row.number, "the row has nothing to import", vec![]);
            }
            match kernel_command(ctx, "db::create", json!({ "model": model.name, "data": full })).await {
                Ok(reply) => Outcome {
                    row: row.number,
                    action: Action::Create,
                    record_id: reply.get("data").and_then(|d| d.get("id")).and_then(Value::as_str).map(str::to_string),
                    message: None,
                    problems: vec![],
                    before: None,
                    after: Some(full),
                },
                Err(error) => Outcome::error(row.number, error.to_string(), vec![]),
            }
        }
        Some(record) => {
            let id = record.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
            let mut changed = Map::new();
            let mut before = Map::new();
            for (name, value) in &data {
                match record.get(name) {
                    Some(current) if same(current, value) => {}
                    other => {
                        before.insert(name.clone(), other.cloned().unwrap_or(Value::Null));
                        changed.insert(name.clone(), value.clone());
                    }
                }
            }
            if changed.is_empty() {
                return Outcome { row: row.number, action: Action::Unchanged, record_id: Some(id), message: None, problems: vec![], before: None, after: None };
            }
            match kernel_command(ctx, "db::update", json!({ "model": model.name, "id": id, "data": changed })).await {
                Ok(_) => Outcome { row: row.number, action: Action::Update, record_id: Some(id), message: None, problems: vec![], before: Some(before), after: Some(changed) },
                Err(error) => Outcome::error(row.number, error.to_string(), vec![]),
            }
        }
    }
}

/// Apply a batch of rows in order. With `stop_on_error` the batch ends at the first row that fails.
pub async fn apply_batch(
    ctx: &PluginHostContext,
    resolved: &Resolved<'_>,
    rows: &[Row],
    stop_on_error: bool,
    links: &mut LinkCache,
) -> Vec<Outcome> {
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let outcome = apply_row(ctx, resolved, row, links).await;
        let failed = outcome.action == Action::Error;
        out.push(outcome);
        if failed && stop_on_error {
            break;
        }
    }
    out
}

/// The text of an error to show for a failed kernel command (the same words a plugin would see).
pub fn describe(error: &HostError) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> ModelDef {
        serde_json::from_value(json!({
            "name": "lead",
            "fields": [
                { "name": "title", "type": "string", "required": true },
                { "name": "email", "type": "string" },
                { "name": "amount", "type": "decimal" },
                { "name": "owner", "type": "link", "target": "party" },
                { "name": "total", "type": "decimal", "compute": "amount * 2" }
            ]
        }))
        .expect("a model")
    }

    fn headers(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn mapping(value: Value) -> Mapping {
        serde_json::from_value(value).expect("a mapping")
    }

    #[test]
    fn a_good_mapping_resolves_column_by_column() {
        let model = model();
        let m = mapping(json!({ "columns": { "Title": "title", "E-mail": "email", "Skip": null }, "key": ["email"] }));
        let r = resolve(&model, &headers(&["Title", "E-mail", "Skip", "Unlisted"]), &m).expect("resolves");
        let names: Vec<Option<&str>> = r.columns.iter().map(|c| c.map(|f| f.name.as_str())).collect();
        assert_eq!(names, vec![Some("title"), Some("email"), None, None]);
    }

    #[test]
    fn a_mapping_that_would_fail_every_row_fails_at_once_and_says_why() {
        let model = model();
        let h = headers(&["Title", "Email", "Total"]);
        let cases = [
            (json!({ "columns": { "Title": "titel" } }), "not a field"),
            (json!({ "columns": { "Title": "title", "Total": "total" } }), "cannot be given a value"),
            (json!({ "columns": { "Title": "title", "Email": "title" } }), "two columns"),
            (json!({ "columns": { "Email": "email" } }), "`title` is required"),
            (json!({ "columns": { "Title": "title" }, "key": ["email"] }), "has no column"),
            (json!({ "columns": { "Title": "title" }, "key": ["nope"] }), "not a field"),
            (json!({ "columns": { "Title": "title" }, "defaults": { "nope": 1 } }), "not a field"),
            (json!({ "columns": { "Title": "title" }, "link_by": { "email": "name" } }), "not a link field"),
            (json!({ "columns": {} }), "no column is mapped"),
        ];
        for (value, expect) in cases {
            let m = mapping(value.clone());
            let error = resolve(&model, &h, &m).expect_err(&format!("{value}"));
            assert!(error.contains(expect), "{value}: {error}");
        }
        // A default fills a required field the file does not have.
        let ok = mapping(json!({ "columns": { "Email": "email" }, "defaults": { "title": "Imported" } }));
        assert!(resolve(&model, &h, &ok).is_ok());
    }

    #[test]
    fn numbers_are_the_same_by_value() {
        assert!(same(&json!(5), &json!(5.0)));
        assert!(same(&json!("a"), &json!("a")));
        assert!(!same(&json!("5"), &json!(5)));
        assert!(!same(&json!(null), &json!("")));
    }
}
