//! What the kernel adds to a write so a model's own rules hold: naming series and `checks`.
//!
//! Both become statements of the write's transaction. A numbered create takes its number from the
//! counter in the same transaction, so a create that fails gives the number back; a check that
//! fails undoes the whole write.

use chrono::Datelike;
use serde_json::{Map, Value as JsonValue};

use super::db::{Extra, validate_ident};
use super::error::HostError;
use crate::data_model::ModelSchema;
use crate::data_model::compute::Rollup;
use crate::data_model::query::compile_filter;
use crate::data_model::runtime::Derivation;
use crate::data_model::sequence::{SEQUENCE_TABLE, Segment, counter_key};

/// Most pieces (text and number) in one series pattern once the date is filled in.
const MAX_SEGMENTS: usize = 9;

/// Bind names are `d<number>` so a transaction can rename them per write. Each series takes a
/// block of this many; a check takes a block for its filter and one name for its message.
const SERIES_BLOCK: usize = 12;
const SERIES_BASE: usize = 700;
const CHECK_FILTER_BASE: usize = 10_000;
const CHECK_MESSAGE_BASE: usize = 20_000;
const UNIQUE_FILTER_BASE: usize = 30_000;
const UNIQUE_MESSAGE_BASE: usize = 40_000;

/// The `CONTENT` expression of a create. Fields of a naming series that the plugin left empty
/// are given the next number; for a model without one it is the plain data.
pub(super) fn create_content(
    schema: &ModelSchema,
    data: &Map<String, JsonValue>,
    injected: Option<(&str, &str)>,
    binds: &mut Vec<(String, JsonValue)>,
    extra: &mut Extra,
) -> Result<String, HostError> {
    let now = chrono::Utc::now();
    let (year, month) = (now.year(), now.month());
    let mut entries = Vec::new();
    for (index, series) in schema.sequences.iter().enumerate() {
        if data.contains_key(&series.column) {
            continue;
        }
        // A series that numbers on a state change waits for it, unless the record is born there.
        if let Some((column, values)) = &series.trigger {
            let born = data
                .get(column)
                .and_then(JsonValue::as_str)
                .is_some_and(|v| values.iter().any(|w| w == v));
            if !born {
                continue;
            }
        }
        validate_ident(&series.column)?;
        let scope = match &series.scope_column {
            Some(column) => Some(
                data.get(column)
                    .and_then(JsonValue::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| {
                        HostError::InvalidPayload(format!(
                            "`{}` is numbered per another field of the record: give it a value",
                            series.field
                        ))
                    })?,
            ),
            None => None,
        };
        let mut key = counter_key(&schema.table, &series.column, series.reset, year, month);
        if let Some(scope) = &scope {
            key = format!("{key}_{scope}");
        }
        let segments = series.pattern.segments(year, month);
        if segments.len() > MAX_SEGMENTS {
            return Err(HostError::InvalidPayload(format!(
                "the numbering of `{}` has too many parts",
                series.field
            )));
        }
        let base = SERIES_BASE + index * SERIES_BLOCK;
        let (key_name, number) = (format!("d{base}"), format!("d{}", base + 1));
        binds.push((key_name.clone(), JsonValue::String(key)));
        extra.before.push(format!(
            "LET ${number} = (UPSERT type::record('{SEQUENCE_TABLE}', ${key_name}) \
             SET counter = (counter ?? 0) + 1 RETURN AFTER)[0].counter;"
        ));
        let value = compose(&series.pattern.segments(year, month), base, &number, binds);
        entries.push(format!("['{}', {value}]", series.column));
    }
    if let Some((column, value_sql)) = injected {
        validate_ident(column)?;
        entries.push(format!("['{column}', <string>{value_sql}]"));
    }
    Ok(if entries.is_empty() {
        "$__data".to_string()
    } else {
        format!(
            "object::from_entries(array::concat(object::entries($__data), [{}]))",
            entries.join(", ")
        )
    })
}

/// SQL that joins the pieces of a number; the counter is in the variable `number`.
fn compose(
    segments: &[Segment],
    base: usize,
    number: &str,
    binds: &mut Vec<(String, JsonValue)>,
) -> String {
    let mut pieces = Vec::new();
    for (offset, segment) in segments.iter().enumerate() {
        match segment {
            Segment::Text(text) => {
                let name = format!("d{}", base + 2 + offset);
                binds.push((name.clone(), JsonValue::String(text.clone())));
                pieces.push(format!("${name}"));
            }
            Segment::Number(width) => pieces.push(format!(
                "string::concat(string::repeat('0', math::max([0, {width} - string::len(<string>${number})])), <string>${number})"
            )),
        }
    }
    format!("string::concat({})", pieces.join(", "))
}

/// The assignments an update adds when it sets a state that numbers the record (a series with
/// `assign_on`). The counter is only taken when the record has no number yet, so reopening and
/// confirming again never uses a second number.
pub(super) fn assign_on_update(
    schema: &ModelSchema,
    set: &Map<String, JsonValue>,
    binds: &mut Vec<(String, JsonValue)>,
    extra: &mut Extra,
) -> Result<Vec<String>, HostError> {
    let now = chrono::Utc::now();
    let (year, month) = (now.year(), now.month());
    let mut assignments = Vec::new();
    for (index, series) in schema.sequences.iter().enumerate() {
        let Some((column, values)) = &series.trigger else { continue };
        let fires = set
            .get(column)
            .and_then(JsonValue::as_str)
            .is_some_and(|v| values.iter().any(|w| w == v));
        if !fires || set.contains_key(&series.column) {
            continue;
        }
        validate_ident(&series.column)?;
        let record = "type::record($__table, $__key)";
        let base = SERIES_BASE + index * SERIES_BLOCK;
        let (key_name, number) = (format!("d{base}"), format!("d{}", base + 1));
        let mut key = counter_key(&schema.table, &series.column, series.reset, year, month);
        let scope_sql = match &series.scope_column {
            Some(scope) => {
                validate_ident(scope)?;
                // The value being set now, else the one the record has.
                Some(match set.get(scope).and_then(JsonValue::as_str) {
                    Some(value) => {
                        key = format!("{key}_{value}");
                        None
                    }
                    None => Some(format!("(SELECT VALUE {scope} FROM ONLY {record})")),
                })
            }
            None => None,
        };
        binds.push((key_name.clone(), JsonValue::String(key)));
        let key_sql = match scope_sql.flatten() {
            Some(sql) => format!("string::concat(${key_name}, '_', <string>{sql})"),
            None => format!("${key_name}"),
        };
        let has_number = format!(
            "(SELECT VALUE {col} FROM ONLY {record}) != NONE",
            col = series.column
        );
        extra.before.push(format!(
            "LET ${number} = IF {has_number} THEN NONE ELSE (UPSERT type::record('{SEQUENCE_TABLE}', {key_sql}) \
             SET counter = (counter ?? 0) + 1 RETURN AFTER)[0].counter END;"
        ));
        let value = compose(&series.pattern.segments(year, month), base, &number, binds);
        assignments.push(format!(
            "{col} = IF {col} != NONE THEN {col} ELSE {value} END",
            col = series.column
        ));
    }
    Ok(assignments)
}

/// After a create or update of one record: refuse the write when the record breaks one of the
/// model's `checks`.
pub(super) fn model_checks(
    schema: &ModelSchema,
    rows: &str,
    binds: &mut Vec<(String, JsonValue)>,
    extra: &mut Extra,
) -> Result<(), HostError> {
    for (index, check) in schema.checks.iter().enumerate() {
        let compiled = compile_filter(
            &check.filter,
            schema,
            &format!("d{}", CHECK_FILTER_BASE + index * 100),
        )
        .map_err(|error| HostError::InvalidPayload(error.to_string()))?;
        if compiled.sql.is_empty() {
            continue;
        }
        let message = format!("d{}", CHECK_MESSAGE_BASE + index);
        binds.extend(compiled.binds);
        binds.push((
            message.clone(),
            JsonValue::String(format!("aether: {}", check.message)),
        ));
        extra.after.push(format!(
            "IF array::len({rows}) > 0 AND array::len((SELECT VALUE id FROM {rows}[0].id WHERE {})) = 0 {{ THROW ${message}; }};",
            compiled.sql
        ));
    }
    unique_checks(schema, rows, binds, extra)?;
    Ok(())
}

/// After a create or update of one record: refuse the write when another record that matches a
/// `unique_when` rule's filter has the same values in its fields, and this one matches it too.
fn unique_checks(
    schema: &ModelSchema,
    rows: &str,
    binds: &mut Vec<(String, JsonValue)>,
    extra: &mut Extra,
) -> Result<(), HostError> {
    for (index, rule) in schema.uniques.iter().enumerate() {
        let compiled = compile_filter(
            &rule.filter,
            schema,
            &format!("d{}", UNIQUE_FILTER_BASE + index * 100),
        )
        .map_err(|error| HostError::InvalidPayload(error.to_string()))?;
        let message = format!("d{}", UNIQUE_MESSAGE_BASE + index);
        binds.extend(compiled.binds);
        binds.push((
            message.clone(),
            JsonValue::String(format!("aether: {}", rule.message)),
        ));
        let mut same = String::new();
        for column in &rule.columns {
            validate_ident(column)?;
            same.push_str(&format!(" AND {column} = {rows}[0].{column}"));
        }
        let (this, others) = if compiled.sql.is_empty() {
            ("true".to_string(), String::new())
        } else {
            (
                format!(
                    "array::len((SELECT VALUE id FROM {rows}[0].id WHERE {})) > 0",
                    compiled.sql
                ),
                format!(" AND ({})", compiled.sql),
            )
        };
        extra.after.push(format!(
            "IF array::len({rows}) > 0 AND {this} AND array::len((SELECT VALUE id FROM {table} WHERE id != {rows}[0].id{same}{others} LIMIT 1)) > 0 {{ THROW ${message}; }};",
            table = schema.table
        ));
    }
    Ok(())
}

/// Statements that fill the fields the kernel calculates for the record `target` (copies through
/// links first, then expressions in the order they are declared). With `rebind`, each one also
/// puts the updated records back into that variable, so what follows sees the result.
fn derived_statements(
    schema: &ModelSchema,
    target: &str,
    rebind: Option<&str>,
    copies_too: bool,
) -> Result<Vec<String>, HostError> {
    let wrap = |set: String| match rebind {
        Some(var) => format!("LET {var} = (UPDATE {target} SET {set} RETURN AFTER);"),
        None => format!("UPDATE {target} SET {set};"),
    };
    let mut statements = Vec::new();
    let copies: Vec<String> = schema
        .derived
        .iter()
        .filter_map(|field| match &field.how {
            Derivation::Related { link, source } if copies_too => {
                Some(format!("{} = (IF {link} != NONE THEN (SELECT VALUE {source} FROM ONLY type::record($parent.{link})) ELSE NONE END)", field.column))
            }
            _ => None,
        })
        .collect();
    if !copies.is_empty() {
        statements.push(wrap(copies.join(", ")));
    }
    for field in &schema.derived {
        if let Derivation::Compute {
            expr,
            scale,
            operands,
        } = &field.how
        {
            let sql = expr
                .to_sql("", *scale, &|name| operands.get(name).cloned(), &|rollup| {
                    rollup_sql(schema, rollup)
                })
                .map_err(|reason| {
                    HostError::InvalidPayload(format!("`{}` {reason}", field.field))
                })?;
            statements.push(wrap(format!("{} = {sql}", field.column)));
        }
    }
    Ok(statements)
}

/// The total over the rows of a child field, for the record being updated.
fn rollup_sql(schema: &ModelSchema, rollup: &Rollup) -> Option<(String, u32)> {
    let child = schema.child(&rollup.child)?;
    let rows = format!(
        "FROM {} WHERE {} = <string>$parent.id",
        child.table, child.inverse_column
    );
    match &rollup.source {
        Some(source) => {
            let number = child.numbers.get(source)?;
            Some((
                format!(
                    "math::sum((SELECT VALUE {col} {rows} AND {col} != NONE))",
                    col = number.column
                ),
                number.scale,
            ))
        }
        None => Some((format!("array::len((SELECT VALUE id {rows}))"), 0)),
    }
}

/// After a create or update of one record: fill the fields the kernel calculates, and bring `$rows`
/// up to date so the checks and the chatter see the result.
pub(super) fn derived_fields(schema: &ModelSchema, extra: &mut Extra) -> Result<(), HostError> {
    let statements = derived_statements(schema, "$rows.id", Some("$rows"), true)?;
    extra.after.splice(0..0, statements);
    Ok(())
}

/// The same for a record written together with its child rows: after the rows are written, the
/// totals over them are worked out again. `rows` is the variable that holds the record.
pub(super) fn derived_again(schema: &ModelSchema, rows: &str) -> Result<Vec<String>, HostError> {
    derived_statements(schema, &format!("{rows}.id"), Some(rows), false)
}

/// After a row of a child field is written on its own (created, changed or deleted): bring the
/// totals of the record it belongs to up to date. `rows` holds the written row.
pub(super) fn refresh_parents(schema: &ModelSchema, extra: &mut Extra) -> Result<(), HostError> {
    for link in &schema.parents {
        validate_ident(&link.inverse_column)?;
        let statements =
            derived_statements(&link.schema, "type::record($rows[0].PARENT)", None, false)?;
        let statements: Vec<String> = statements
            .into_iter()
            .map(|s| s.replace("PARENT", &link.inverse_column))
            .collect();
        if statements.is_empty() {
            continue;
        }
        extra.after.push(format!(
            "IF array::len($rows) > 0 AND $rows[0].{col} != NONE {{ {body} }};",
            col = link.inverse_column,
            body = statements.join(" ")
        ));
    }
    Ok(())
}

/// After an update of one record: the copies other models keep of the fields that changed are
/// refilled (`related` fields are copies; without this they would only catch up when their own
/// record is next written). Only touched when one of the copied source columns was set or cleared.
pub(super) fn propagate_copies(
    schema: &ModelSchema,
    touched: &[&str],
    extra: &mut Extra,
) -> Result<(), HostError> {
    for dependent in &schema.dependents {
        let copies: Vec<&(String, String)> = dependent
            .copies
            .iter()
            .filter(|(_, source)| touched.contains(&source.as_str()))
            .collect();
        if copies.is_empty() {
            continue;
        }
        validate_ident(&dependent.table)?;
        validate_ident(&dependent.link_column)?;
        let mut sets = Vec::new();
        for (copy, source) in copies {
            validate_ident(copy)?;
            validate_ident(source)?;
            sets.push(format!("{copy} = $rows[0].{source}"));
        }
        extra.after.push(format!(
            "IF array::len($rows) > 0 {{ UPDATE {table} SET {sets} WHERE {link} = <string>$rows[0].id; }};",
            table = dependent.table,
            sets = sets.join(", "),
            link = dependent.link_column
        ));
    }
    Ok(())
}
