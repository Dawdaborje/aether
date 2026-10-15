//! `report::query` and `report::export`.
//!
//! A report reads one granted model (the plugin's own, or `dependency.model` when that plugin is
//! a dependency) through the same filters, record rules and field hiding as `db::aggregate`.
//! The file export does not read the database: it renders rows the plugin already has.

use std::collections::HashSet;

use aether_report::{
    AvgDaysTo, Dimension, Fill, Measure, ReportQuery, export_table, normalize_preview_rows,
    parse_export,
};
use serde_json::{Map, Value as JsonValue};

use crate::data_model::query::{Columns, Unchecked, compile_filter};
use crate::data_model::{Filter, ModelSchema};

use super::context::{ModelGrant, PluginHostContext};
use super::db::{Access, Extra, guarded_condition, require_model, run_audited, validate_ident};
use super::error::HostError;
use super::guard;

const PREVIEW_KEY: &str = "ui.report.preview_rows";

pub async fn report_query(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("report::query")?;
    let query = ReportQuery::parse(payload)
        .map_err(|error| HostError::InvalidPayload(error.to_string()))?;
    let grant = require_model(ctx, &query.model, false)?;
    let Some((condition, mut binds, hidden)) =
        guarded_condition(ctx, grant, &query.model, &query.filter).await?
    else {
        return Ok(empty_answer(&query));
    };
    guard::check_unreadable(&hidden, query.fields(), &query.model)?;
    check_grains(grant, &query)?;

    let mut select = Vec::new();
    let mut group = Vec::new();
    for dimension in &query.dimensions {
        let expression = dimension_sql(grant, dimension)?;
        select.push(format!("{expression} AS {}", dimension.field));
        group.push(expression);
    }
    let mut decimals = Vec::new();
    let mut measures = Vec::new();
    for (index, (name, measure)) in query.measures.iter().enumerate() {
        let (sql, extra) = measure_sql(grant, measure, &format!("m{index}"))?;
        binds.extend(extra);
        select.push(format!("{sql} AS {name}"));
        measures.push(sql);
        if let Some(scale) = measure_scale(grant, measure) {
            decimals.push((name.clone(), scale));
        }
    }

    let mut body = format!(
        "LET $groups = (SELECT {} FROM type::table($__table)",
        select.join(", ")
    );
    if !condition.is_empty() {
        body.push_str(" WHERE ");
        body.push_str(&condition);
    }
    if group.is_empty() {
        body.push_str(" GROUP ALL");
    } else {
        body.push_str(" GROUP BY ");
        body.push_str(&group.join(", "));
    }
    if let Some(order) = &query.order {
        let (name, direction) = match order.strip_prefix('-') {
            Some(name) => (name, " DESC"),
            None => (order.as_str(), ""),
        };
        body.push_str(&format!(" ORDER BY {name}{direction}"));
    }
    body.push_str(&format!(" LIMIT {});", query.limit));

    // The same measure SQL, so the ungrouped totals share the binds taken above.
    let total_select: Vec<String> = query
        .measures
        .iter()
        .zip(measures.iter())
        .map(|((name, _), sql)| format!("{sql} AS {name}"))
        .collect();
    body.push_str(&format!(
        "\nLET $total = (SELECT {} FROM type::table($__table)",
        total_select.join(", ")
    ));
    if !condition.is_empty() {
        body.push_str(" WHERE ");
        body.push_str(&condition);
    }
    body.push_str(" GROUP ALL);");

    let (fill_label, fill_sql) = match &query.fill {
        Some(fill) => {
            let sql = fill_sql(ctx, fill, &mut binds).await?;
            (Some(fill.label.clone()), sql)
        }
        None => (None, "LET $fill = [];".to_string()),
    };
    body.push('\n');
    body.push_str(&fill_sql);
    body.push_str("\nLET $rows = [{ groups: $groups, total: $total, fill: $fill }];");

    // Everything up to the preview cap depends only on the data and the statement, so that is
    // what Mantle keeps; the cap (an organization setting) is applied after.
    let hash = super::db::query_hash(&grant.table, &body, &binds, &hidden);
    // The statements before `LET $rows` run ahead of the audit insert as `before`, which is what
    // tells the runner where the transaction's `RETURN` sits (it counts statements).
    let (head, tail) = body.rsplit_once('\n').unwrap_or(("", body.as_str()));
    let report_extra = || Extra {
        before: head.lines().map(str::to_string).collect(),
        after: Vec::new(),
    };
    // The fill model's table feeds the report too: a stage added there changes the answer.
    let mut tables = vec![grant.table.as_str()];
    if let Some(fill) = &query.fill
        && let Ok(fill_grant) = require_model(ctx, &fill.model, false)
    {
        tables.push(fill_grant.table.as_str());
    }
    let answer = super::db::cached_read(
        ctx,
        &query.model,
        &grant.table,
        &tables,
        "report",
        &hash,
        || async {
            let rows = run_audited(
                ctx,
                Access {
                    operation: "read",
                    model: query.model.clone(),
                    table: &grant.table,
                },
                tail,
                "[]",
                binds,
                report_extra(),
            )
            .await?;
            let n = rows.len();
            let bundle = rows.into_iter().next().unwrap_or(JsonValue::Null);
            let groups = array_of(bundle.get("groups"));
            let total_rows = array_of(bundle.get("total"));
            let fill_rows = array_of(bundle.get("fill"));

            let mut groups = present_rows(groups, &decimals);
            let total_rows = present_rows(total_rows, &decimals);
            let mut total = total_rows
                .into_iter()
                .next()
                .unwrap_or_else(|| zero_measures(&query));
            strip_id(&mut total);

            if let (Some(label), Some(dimension)) =
                (fill_label.as_deref(), query.dimensions.first())
            {
                groups = merge_fill(&dimension.field, label, groups, fill_rows, &query);
                if let Some(order) = &query.order {
                    sort_rows(&mut groups, order);
                }
                groups.truncate(query.limit as usize);
            }
            Ok((
                serde_json::json!({ "groups": groups, "total": total }),
                Vec::new(),
                n,
            ))
        },
    )
    .await?;
    let mut groups: Vec<JsonValue> = array_of(answer.get("groups"));
    let total = answer.get("total").cloned().unwrap_or(JsonValue::Null);

    let total_groups = groups.len() as u64;
    let (shown, preview_rows) = if query.preview {
        let cap = preview_limit(ctx).await;
        groups.truncate(cap as usize);
        (groups.len() as u64, Some(cap))
    } else {
        (total_groups, None)
    };

    let mut data = serde_json::json!({
        "rows": groups,
        "total": total,
        "shown": shown,
        "total_groups": total_groups,
    });
    if let Some(cap) = preview_rows {
        data["preview_rows"] = JsonValue::from(cap);
    }
    Ok(serde_json::json!({ "ok": true, "data": data }))
}

pub async fn report_export(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("report::export")?;
    let table =
        parse_export(payload).map_err(|error| HostError::InvalidPayload(error.to_string()))?;
    let file =
        export_table(&table).map_err(|error| HostError::InvalidPayload(error.to_string()))?;
    let data = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &file.bytes);
    Ok(serde_json::json!({
        "ok": true,
        "data": {
            "filename": file.filename,
            "content_type": file.content_type,
            "data": data,
        }
    }))
}

fn empty_answer(query: &ReportQuery) -> JsonValue {
    serde_json::json!({
        "ok": true,
        "data": {
            "rows": [],
            "total": zero_measures(query),
            "shown": 0,
            "total_groups": 0,
            "preview_rows": if query.preview { JsonValue::from(aether_report::PREVIEW_DEFAULT) } else { JsonValue::Null },
        }
    })
}

fn check_grains(grant: &ModelGrant, query: &ReportQuery) -> Result<(), HostError> {
    let Some(schema) = &grant.schema else {
        return Ok(());
    };
    for dimension in &query.dimensions {
        if dimension.grain.is_some() {
            require_temporal(schema, &dimension.field, "a date grain")?;
        }
    }
    for (name, measure) in &query.measures {
        if let Measure::AvgDays { from, to } = measure {
            require_temporal(schema, from, "avg_days")?;
            if let AvgDaysTo::Field(field) = to {
                require_temporal(schema, field, "avg_days")?;
            }
            let _ = name;
        }
    }
    Ok(())
}

fn require_temporal(schema: &ModelSchema, field: &str, what: &str) -> Result<(), HostError> {
    let kind = schema.field_kind(field)?;
    if kind == "date" || kind == "datetime" {
        Ok(())
    } else {
        Err(HostError::InvalidPayload(format!(
            "`{field}` is a {kind}, so it cannot be used for {what}"
        )))
    }
}

fn dimension_sql(grant: &ModelGrant, dimension: &Dimension) -> Result<String, HostError> {
    let column = column_expr(grant, &dimension.field)?;
    Ok(match dimension.grain {
        Some(grain) => format!(
            "time::format(type::datetime({column}), \"{}\")",
            grain.format()
        ),
        None => column,
    })
}

fn column_expr(grant: &ModelGrant, field: &str) -> Result<String, HostError> {
    match &grant.schema {
        Some(schema) => Ok(schema.column(field)?),
        None => {
            validate_ident(field)?;
            Ok(field.to_string())
        }
    }
}

fn measure_sql(
    grant: &ModelGrant,
    measure: &Measure,
    prefix: &str,
) -> Result<(String, Vec<(String, JsonValue)>), HostError> {
    let number =
        |field: &str, func: &str| -> Result<(String, Vec<(String, JsonValue)>), HostError> {
            let column = column_expr(grant, field)?;
            Ok((format!("{func}({column})"), Vec::new()))
        };
    match measure {
        Measure::Count => Ok(("count()".to_string(), Vec::new())),
        Measure::Sum(field) => number(field, "math::sum"),
        Measure::Avg(field) => number(field, "math::mean"),
        Measure::Min(field) => number(field, "math::min"),
        Measure::Max(field) => number(field, "math::max"),
        Measure::CountIf(filter) => {
            let parsed = Filter::parse(filter)?;
            let compiled = match &grant.schema {
                Some(schema) => compile_filter(&parsed, schema.as_ref(), prefix)?,
                None => compile_filter(&parsed, &Unchecked, prefix)?,
            };
            let condition = if compiled.sql.is_empty() {
                "true".to_string()
            } else {
                compiled.sql
            };
            Ok((
                format!("count(IF {condition} THEN true ELSE NONE END)"),
                compiled.binds,
            ))
        }
        Measure::AvgDays { from, to } => {
            let start = column_expr(grant, from)?;
            let end = match to {
                AvgDaysTo::Now => "time::now()".to_string(),
                AvgDaysTo::Field(field) => column_expr(grant, field)?,
            };
            Ok((
                format!(
                    "math::mean(IF {start} = NONE OR {end} = NONE THEN NONE ELSE (time::unix(type::datetime({end})) - time::unix(type::datetime({start}))) / 86400 END)"
                ),
                Vec::new(),
            ))
        }
    }
}

fn measure_scale(grant: &ModelGrant, measure: &Measure) -> Option<u32> {
    let field = match measure {
        Measure::Sum(field) | Measure::Avg(field) | Measure::Min(field) | Measure::Max(field) => {
            field.as_str()
        }
        _ => return None,
    };
    grant.schema.as_ref()?.scale(field)
}

async fn fill_sql(
    ctx: &PluginHostContext,
    fill: &Fill,
    binds: &mut Vec<(String, JsonValue)>,
) -> Result<String, HostError> {
    let grant = require_model(ctx, &fill.model, false)?;
    let Some((condition, fill_binds, hidden)) =
        guarded_condition(ctx, grant, &fill.model, &fill.filter).await?
    else {
        return Ok("LET $fill = [];".to_string());
    };
    guard::check_unreadable(
        &hidden,
        [fill.key.as_str(), fill.label.as_str()],
        &fill.model,
    )?;
    let key = column_expr(grant, &fill.key)?;
    let label = column_expr(grant, &fill.label)?;
    let mut fill_binds = fill_binds;
    let mut condition = condition;
    // The fact query already binds `$f*` and `$r*`.
    retarget(&mut condition, &mut fill_binds, "f", "g");
    retarget(&mut condition, &mut fill_binds, "r", "q");
    for (name, value) in fill_binds {
        if name == "__table" {
            binds.push(("__fill".to_string(), value));
        } else {
            binds.push((name, value));
        }
    }
    let mut sql =
        format!("LET $fill = (SELECT {key} AS __key, {label} AS __label FROM type::table($__fill)");
    if !condition.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&condition);
    }
    sql.push_str(" LIMIT 1000);");
    Ok(sql)
}

/// Rename `$old0` to `$new0` in a condition and its binds. Longer names go first so `$f10`
/// is not rewritten as `$g10` when `$f1` is replaced.
fn retarget(sql: &mut String, binds: &mut [(String, JsonValue)], old: &str, new: &str) {
    let mut names: Vec<String> = binds
        .iter()
        .map(|(name, _)| name.clone())
        .filter(|name| {
            name.starts_with(old) && name[old.len()..].chars().all(|c| c.is_ascii_digit())
        })
        .collect();
    names.sort_by_key(|name| std::cmp::Reverse(name.len()));
    for name in names {
        let renamed = format!("{new}{}", &name[old.len()..]);
        *sql = sql.replace(&format!("${name}"), &format!("${renamed}"));
        if let Some(bind) = binds.iter_mut().find(|(current, _)| current == &name) {
            bind.0 = renamed;
        }
    }
}

fn array_of(value: Option<&JsonValue>) -> Vec<JsonValue> {
    value
        .and_then(JsonValue::as_array)
        .cloned()
        .unwrap_or_default()
}

fn present_rows(rows: Vec<JsonValue>, decimals: &[(String, u32)]) -> Vec<JsonValue> {
    rows.into_iter()
        .map(|mut row| {
            for (name, scale) in decimals {
                if let Some(value) = row.get(name) {
                    row[name.as_str()] = crate::data_model::decimal::present(value, *scale);
                }
            }
            if let JsonValue::Object(object) = &mut row {
                object.remove("id");
            }
            row
        })
        .collect()
}

fn strip_id(row: &mut JsonValue) {
    if let JsonValue::Object(object) = row {
        object.remove("id");
    }
}

fn zero_measures(query: &ReportQuery) -> JsonValue {
    let mut object = Map::new();
    for (name, measure) in &query.measures {
        let value = match measure {
            Measure::Count | Measure::CountIf(_) | Measure::Sum(_) => JsonValue::from(0),
            Measure::Avg(_) | Measure::Min(_) | Measure::Max(_) | Measure::AvgDays { .. } => {
                JsonValue::Null
            }
        };
        object.insert(name.clone(), value);
    }
    JsonValue::Object(object)
}

fn merge_fill(
    dimension: &str,
    label: &str,
    groups: Vec<JsonValue>,
    fill_rows: Vec<JsonValue>,
    query: &ReportQuery,
) -> Vec<JsonValue> {
    let mut by_key: Vec<(String, JsonValue)> = groups
        .into_iter()
        .map(|row| (json_key(row.get(dimension)), row))
        .collect();
    let mut ordered = Vec::new();
    let mut seen = HashSet::new();
    for fill in fill_rows {
        let key = json_key(fill.get("__key"));
        if !seen.insert(key.clone()) {
            continue;
        }
        let mut row = by_key
            .iter()
            .position(|(existing, _)| existing == &key)
            .map(|index| by_key.remove(index).1)
            .unwrap_or_else(|| {
                let mut blank = zero_measures(query);
                if let JsonValue::Object(object) = &mut blank {
                    object.insert(dimension.to_string(), JsonValue::String(key.clone()));
                }
                blank
            });
        if let JsonValue::Object(object) = &mut row {
            if let Some(text) = fill.get("__label") {
                object.insert(label.to_string(), text.clone());
            }
            object.remove("id");
        }
        ordered.push(row);
    }
    for (_, row) in by_key {
        ordered.push(row);
    }
    ordered
}

fn json_key(value: Option<&JsonValue>) -> String {
    match value {
        Some(JsonValue::String(text)) => text.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

fn sort_rows(rows: &mut [JsonValue], order: &str) {
    let (name, desc) = match order.strip_prefix('-') {
        Some(name) => (name, true),
        None => (order, false),
    };
    rows.sort_by(|left, right| {
        let order = cmp_json(left.get(name), right.get(name));
        if desc { order.reverse() } else { order }
    });
}

fn cmp_json(left: Option<&JsonValue>, right: Option<&JsonValue>) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (left, right) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(JsonValue::Number(a)), Some(JsonValue::Number(b))) => a
            .as_f64()
            .partial_cmp(&b.as_f64())
            .unwrap_or(Ordering::Equal),
        (Some(JsonValue::String(a)), Some(JsonValue::String(b))) => a.cmp(b),
        (Some(a), Some(b)) => a.to_string().cmp(&b.to_string()),
    }
}

async fn preview_limit(ctx: &PluginHostContext) -> u32 {
    if let Some(value) = read_setting(&ctx.db, "settings_items").await {
        return normalize_preview_rows(Some(&value));
    }
    if let Ok(services) = ctx.services() {
        if let Some(core) = &services.core {
            if let Some(value) = read_setting(core, "gl_settings_items").await {
                return normalize_preview_rows(Some(&value));
            }
        }
    }
    aether_report::PREVIEW_DEFAULT
}

async fn read_setting(db: &crate::state::Db, table: &str) -> Option<JsonValue> {
    validate_ident(table).ok()?;
    let mut response = db
        .query(&format!(
            "SELECT s_value FROM {table} WHERE s_key = $key LIMIT 1;"
        ))
        .bind(("key", PREVIEW_KEY.to_string()))
        .await
        .ok()?;
    let rows: Vec<JsonValue> = response.take(0).ok()?;
    rows.into_iter()
        .next()
        .and_then(|row| row.get("s_value").cloned())
}
