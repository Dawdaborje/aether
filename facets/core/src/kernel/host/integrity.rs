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
use crate::data_model::runtime::Derivation;
use crate::data_model::query::compile_filter;
use crate::data_model::sequence::{SEQUENCE_TABLE, Segment, counter_key};

/// Most pieces (text and number) in one series pattern once the date is filled in.
const MAX_SEGMENTS: usize = 9;

/// Bind names are `d<number>` so a transaction can rename them per write. Each series takes a
/// block of this many; a check takes a block for its filter and one name for its message.
const SERIES_BLOCK: usize = 12;
const SERIES_BASE: usize = 700;
const CHECK_FILTER_BASE: usize = 10_000;
const CHECK_MESSAGE_BASE: usize = 20_000;

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
        validate_ident(&series.column)?;
        let segments = series.pattern.segments(year, month);
        if segments.len() > MAX_SEGMENTS {
            return Err(HostError::InvalidPayload(format!("the numbering of `{}` has too many parts", series.field)));
        }
        let base = SERIES_BASE + index * SERIES_BLOCK;
        let (key_name, number) = (format!("d{base}"), format!("d{}", base + 1));
        binds.push((key_name.clone(), JsonValue::String(counter_key(&schema.table, &series.column, series.reset, year, month))));
        extra.before.push(format!(
            "LET ${number} = (UPSERT type::record('{SEQUENCE_TABLE}', ${key_name}) \
             SET counter = (counter ?? 0) + 1 RETURN AFTER)[0].counter;"
        ));
        let mut pieces = Vec::new();
        for (offset, segment) in segments.into_iter().enumerate() {
            match segment {
                Segment::Text(text) => {
                    let name = format!("d{}", base + 2 + offset);
                    binds.push((name.clone(), JsonValue::String(text)));
                    pieces.push(format!("${name}"));
                }
                Segment::Number(width) => pieces.push(format!(
                    "string::concat(string::repeat('0', math::max([0, {width} - string::len(<string>${number})])), <string>${number})"
                )),
            }
        }
        entries.push(format!("['{}', string::concat({})]", series.column, pieces.join(", ")));
    }
    if let Some((column, value_sql)) = injected {
        validate_ident(column)?;
        entries.push(format!("['{column}', <string>{value_sql}]"));
    }
    Ok(if entries.is_empty() {
        "$__data".to_string()
    } else {
        format!("object::from_entries(array::concat(object::entries($__data), [{}]))", entries.join(", "))
    })
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
        let compiled = compile_filter(&check.filter, schema, &format!("d{}", CHECK_FILTER_BASE + index * 100))
            .map_err(|error| HostError::InvalidPayload(error.to_string()))?;
        if compiled.sql.is_empty() {
            continue;
        }
        let message = format!("d{}", CHECK_MESSAGE_BASE + index);
        binds.extend(compiled.binds);
        binds.push((message.clone(), JsonValue::String(format!("aether: {}", check.message))));
        extra.after.push(format!(
            "IF array::len({rows}) > 0 AND array::len((SELECT VALUE id FROM {rows}[0].id WHERE {})) = 0 {{ THROW ${message}; }};",
            compiled.sql
        ));
    }
    Ok(())
}

/// Statements that fill the fields the kernel calculates for the record `target` (copies through
/// links first, then expressions in the order they are declared). With `rebind`, each one also
/// puts the updated records back into that variable, so what follows sees the result.
fn derived_statements(schema: &ModelSchema, target: &str, rebind: Option<&str>, copies_too: bool) -> Result<Vec<String>, HostError> {
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
        if let Derivation::Compute { expr, scale, operands } = &field.how {
            let sql = expr
                .to_sql("", *scale, &|name| operands.get(name).cloned(), &|rollup| rollup_sql(schema, rollup))
                .map_err(|reason| HostError::InvalidPayload(format!("`{}` {reason}", field.field)))?;
            statements.push(wrap(format!("{} = {sql}", field.column)));
        }
    }
    Ok(statements)
}

/// The total over the rows of a child field, for the record being updated.
fn rollup_sql(schema: &ModelSchema, rollup: &Rollup) -> Option<(String, u32)> {
    let child = schema.child(&rollup.child)?;
    let rows = format!("FROM {} WHERE {} = <string>$parent.id", child.table, child.inverse_column);
    match &rollup.source {
        Some(source) => {
            let number = child.numbers.get(source)?;
            Some((format!("math::sum((SELECT VALUE {col} {rows} AND {col} != NONE))", col = number.column), number.scale))
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
        let statements = derived_statements(&link.schema, "type::record($rows[0].PARENT)", None, false)?;
        let statements: Vec<String> = statements.into_iter().map(|s| s.replace("PARENT", &link.inverse_column)).collect();
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
