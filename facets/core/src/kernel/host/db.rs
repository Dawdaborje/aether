use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as JsonValue};

use crate::data_model::query::{Unchecked, compile_filter};
use crate::data_model::{Aggregate, Filter};

use super::context::{ModelGrant, PluginHostContext};
use super::guard;
use crate::data_model::Operation;
use super::error::HostError;

#[derive(Debug, Deserialize)]
pub struct GetRequest {
    pub model: String,
    pub id: String,
    /// Child fields whose rows to include.
    #[serde(default)]
    pub expand: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct FindRequest {
    pub model: String,
    #[serde(default)]
    pub filter: Map<String, JsonValue>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub offset: Option<u32>,
    #[serde(default)]
    pub order: Option<String>,
    /// Child fields whose rows to include.
    #[serde(default)]
    pub expand: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRequest {
    pub model: String,
    pub data: Map<String, JsonValue>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRequest {
    pub model: String,
    pub id: String,
    pub data: Map<String, JsonValue>,
}

#[derive(Debug, Deserialize)]
pub struct IncrementRequest {
    pub model: String,
    pub id: String,
    pub field: String,
    /// What to add; negative subtracts. Defaults to one. A decimal field takes text (`"0.5"`)
    /// or a number.
    #[serde(default = "one")]
    pub by: JsonValue,
}

fn one() -> JsonValue {
    JsonValue::from(1)
}

#[derive(Debug, Deserialize)]
pub struct DeleteRequest {
    pub model: String,
    pub id: String,
}

#[derive(Debug, Serialize)]
pub struct DbResponse {
    pub ok: bool,
    pub data: JsonValue,
}

pub(super) fn require_model<'a>(
    ctx: &'a PluginHostContext,
    name: &str,
    write: bool,
) -> Result<&'a ModelGrant, HostError> {
    let grant = ctx
        .model(name)
        .ok_or_else(|| HostError::ModelDenied(name.to_string()))?;
    if write {
        if !grant.can_write {
            return Err(HostError::ModelPermission(name.to_string(), "write"));
        }
    } else if !grant.can_read {
        return Err(HostError::ModelPermission(name.to_string(), "read"));
    }
    validate_ident(&grant.table)?;
    if RESERVED_TABLES.contains(&grant.table.as_str()) {
        return Err(HostError::ReservedTable(grant.table.clone()));
    }
    Ok(grant)
}

pub(super) fn validate_ident(name: &str) -> Result<(), HostError> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(HostError::InvalidPayload(format!(
            "invalid identifier `{name}`"
        )));
    }
    Ok(())
}

fn validate_field_name(name: &str) -> Result<(), HostError> {
    validate_ident(name)
}

pub(super) fn parse_req<T: serde::de::DeserializeOwned>(payload: &JsonValue) -> Result<T, HostError> {
    serde_json::from_value(payload.clone())
        .map_err(|e| HostError::InvalidPayload(e.to_string()))
}

/// Rows a single `db::find` may return, whatever limit the plugin asks for.
pub const MAX_ROWS_PER_FIND: u32 = 1000;

/// Tables the kernel owns in every organization database. A plugin cannot map
/// a model onto one of them.
const RESERVED_TABLES: &[&str] = &[
    "aether_sequence",
    "bridge_configs",
    "chatter_followers",
    "chatter_messages",
    "data_access",
    "group_roles",
    "groups",
    "installed_plugin_depends_on",
    "installed_plugins",
    "invitations",
    "media",
    "model_schema",
    "notification_reads",
    "notifications",
    "org_user_groups",
    "org_user_roles",
    "org_users",
    "organizations",
    "page_visits",
    "permissions",
    "plugin_calls",
    "role_permissions",
    "roles",
    "scheduled_tasks",
    "jobs",
    "schema_migrations",
    "settings_group_items",
    "settings_groups",
    "settings_items",
    "storage_backends",
    "ui_theme_config",
    "ui_themes",
    "visitors",
];

/// Index of the `RETURN $rows` statement in a plain audited transaction: `BEGIN`, the work, the
/// audit insert, then the return. Each statement [`Extra`] adds moves it by one.
const RETURN_INDEX: usize = 3;

/// Statements run in the same transaction as the work, before it and after the audit row.
/// Every entry must be exactly one statement, so the return index can be counted.
#[derive(Default)]
pub(super) struct Extra {
    pub(super) before: Vec<String>,
    pub(super) after: Vec<String>,
}

impl Extra {
    fn return_index(&self) -> usize {
        RETURN_INDEX + self.before.len() + self.after.len()
    }
}

/// What a model's chatter records about a write, as statements for the same transaction as the
/// write itself, so a change cannot be made without its record (and a failed write leaves none).
/// Nothing is added unless the model switched chatter on.
fn chatter_extra(grant: &ModelGrant, operation: &str) -> Result<Extra, HostError> {
    let Some(schema) = &grant.schema else { return Ok(Extra::default()) };
    let Some(chatter) = &schema.chatter else { return Ok(Extra::default()) };
    let line = "CREATE chatter_messages SET ulid = rand::ulid(), model_id = $__table, \
                author = $__cm_author, plugin = $__audit_plugin";
    let mut extra = Extra::default();
    match operation {
        "create" => {
            let key = "<string> record::id($rows[0].id)";
            extra.after.push(format!("{line}, record_key = {key}, kind = 'system', body = 'created';"));
            if chatter.followers {
                extra.after.push(format!(
                    "IF $__cm_author != 'system' {{ CREATE chatter_followers SET model_id = $__table, \
                     record_key = {key}, actor = $__cm_author, reason = 'creator'; }};"
                ));
            }
        }
        "update" if !schema.tracked.is_empty() => {
            let mut ids = Vec::new();
            for id in &schema.tracked {
                validate_field_name(id)?;
                ids.push(format!("'{id}'"));
            }
            let ids = ids.join(", ");
            extra.before.push("LET $__before = (SELECT * FROM ONLY type::record($__table, $__key));".into());
            extra.after.push(format!(
                "LET $__changed = array::filter([{ids}], |$k| $__before[$k] != $rows[0][$k]);"
            ));
            extra.after.push(format!(
                "IF array::len($__changed) > 0 {{ {line}, record_key = $__key, kind = 'change', \
                 before = object::from_entries(array::map($__changed, |$k| [$k, $__before[$k]])), \
                 after = object::from_entries(array::map($__changed, |$k| [$k, $rows[0][$k]])); }};"
            ));
        }
        "delete" => {
            extra.after.push(
                "UPDATE chatter_messages SET record_deleted_at = time::now() \
                 WHERE model_id = $__table AND record_key = $__key AND record_deleted_at = NONE;"
                    .into(),
            );
            extra.after.push("DELETE chatter_followers WHERE model_id = $__table AND record_key = $__key;".into());
        }
        _ => {}
    }
    Ok(extra)
}

pub(super) struct Access<'a> {
    pub(super) operation: &'static str,
    pub(super) model: String,
    pub(super) table: &'a str,
}

/// Run `body` (which must `LET $rows = …;`) and record the access in
/// `data_access` inside one transaction. If the audit row cannot be written,
/// nothing is applied and the call fails. `ids` is the SurrealQL expression for
/// the record ids touched.
pub(super) async fn run_audited(
    ctx: &PluginHostContext,
    access: Access<'_>,
    body: &str,
    ids: &str,
    binds: Vec<(String, JsonValue)>,
    extra: Extra,
) -> Result<Vec<JsonValue>, HostError> {
    let (before, after) = (extra.before.join("\n"), extra.after.join("\n"));
    let surql = format!(
        r#"
        BEGIN TRANSACTION;
        {before}
        {body}
        CREATE data_access SET
            request_id = $__audit_request,
            actor_type = $__audit_actor_type,
            actor_id = $__audit_actor_id,
            plugin = $__audit_plugin,
            function_name = $__audit_function,
            model = $__audit_model,
            table_name = $__audit_table,
            operation = $__audit_operation,
            record_ids = {ids},
            record_count = array::len($rows),
            ip = $__audit_ip;
        {after}
        RETURN $rows;
        COMMIT TRANSACTION;
        "#
    );
    let mut query = ctx
        .db
        .query(surql)
        .bind(("__audit_request", ctx.audit.request_id.clone()))
        .bind(("__audit_actor_type", ctx.audit.actor.kind().to_string()))
        .bind(("__audit_actor_id", ctx.audit.actor.id().map(str::to_string)))
        .bind(("__audit_plugin", ctx.plugin_name.clone()))
        .bind(("__audit_function", ctx.function.clone()))
        .bind(("__audit_model", access.model))
        .bind(("__audit_table", access.table.to_string()))
        .bind(("__audit_operation", access.operation.to_string()))
        .bind(("__audit_ip", ctx.audit.ip.clone()))
        .bind(("__cm_author", ctx.audit.actor.id().unwrap_or("system").to_string()));
    for (name, value) in binds {
        query = query.bind((name, value));
    }
    let mut response = checked(query.await.map_err(map_db_error)?)?;
    Ok(response.take(extra.return_index())?)
}

/// A rule the kernel enforces inside a write (`THROW 'aether: …'`) reaches the plugin as a plain
/// message about its own request, not as a database failure.
pub(super) fn map_db_error(error: surrealdb::Error) -> HostError {
    let text = error.to_string();
    if let Some((_, message)) = text.split_once("aether-denied: ") {
        return HostError::Denied(message.trim_end_matches(['\'', '"']).to_string());
    }
    match text.split_once("aether: ") {
        Some((_, message)) => HostError::InvalidPayload(message.trim_end_matches(['\'', '"']).to_string()),
        None => HostError::Db(error),
    }
}

/// A response with every statement succeeded. When one fails inside a transaction the others
/// come back as "not executed", so the statement that actually failed is the one to report.
fn checked(mut response: surrealdb::IndexedResults) -> Result<surrealdb::IndexedResults, HostError> {
    let errors = response.take_errors();
    if errors.is_empty() {
        return Ok(response);
    }
    let mut ordered: Vec<(usize, surrealdb::Error)> = errors.into_iter().collect();
    ordered.sort_by_key(|(index, _)| *index);
    let culprit = ordered
        .iter()
        .position(|(_, error)| !error.to_string().contains("not executed"))
        .unwrap_or(0);
    let (_, error) = ordered.swap_remove(culprit);
    Err(map_db_error(error))
}

/// Before a write to one record: refuse it, and undo nothing because nothing has happened yet,
/// unless the record is one the model's rules let the caller change.
fn add_record_check(
    extra: &mut Extra,
    binds: &mut Vec<(String, JsonValue)>,
    scope: guard::Scope,
    model: &str,
    what: &str,
) {
    if scope.sql.is_empty() {
        return;
    }
    extra.before.insert(
        0,
        format!(
            "IF array::len((SELECT VALUE id FROM type::record($__table, $__key) WHERE {})) = 0 \
             {{ THROW 'aether-denied: you may not {what} this `{model}` record'; }};",
            scope.sql
        ),
    );
    binds.extend(scope.binds);
}

pub(super) fn record_binds(grant: &ModelGrant, id: &str) -> Vec<(String, JsonValue)> {
    vec![
        ("__table".to_string(), JsonValue::String(grant.table.clone())),
        (
            "__key".to_string(),
            JsonValue::String(strip_table_prefix(id, &grant.table).to_string()),
        ),
    ]
}

pub(super) const RECORD_ID: &str = "[type::record($__table, $__key)]";

/// One write, ready to run: the statement, what it binds, and the audit facts about it. A single
/// command runs one plan in its own transaction; `db::transaction` runs several in one.
pub(super) struct Plan<'a> {
    pub(super) access: Access<'a>,
    pub(super) grant: &'a ModelGrant,
    pub(super) body: String,
    pub(super) ids: String,
    pub(super) binds: Vec<(String, JsonValue)>,
    pub(super) extra: Extra,
}

/// How a write differs when it is one row of a record's child field.
#[derive(Default)]
pub(super) struct Options {
    /// The link of the row that points at its record, filled in by the kernel from this SurrealQL
    /// value (the record's id, which is not known yet when both are created together).
    pub(super) inject: Option<Injection>,
    /// A write with nothing to set is fine (the kernel adds what is missing).
    pub(super) allow_empty: bool,
    /// The record's totals are brought up to date by the caller, once, after all the rows.
    pub(super) skip_parent_refresh: bool,
    /// The model's checks are run by the caller, after all the rows are written.
    pub(super) defer_checks: bool,
}

pub(super) struct Injection {
    pub(super) field: String,
    pub(super) column: String,
    pub(super) value_sql: String,
}

async fn run_plan(ctx: &PluginHostContext, plan: Plan<'_>) -> Result<JsonValue, HostError> {
    let grant = plan.grant;
    let rows = run_audited(ctx, plan.access, &plan.body, &plan.ids, plan.binds, plan.extra).await?;
    let (hidden, _) = guard::field_limits(ctx, grant).await?;
    Ok(serde_json::json!({ "ok": true, "data": rows.into_iter().next().map(|row| guard::strip(&hidden, decode(grant, row))) }))
}

/// A record as the plugin sees it: field names, not the ids it is stored under.
pub(super) fn decode(grant: &ModelGrant, row: JsonValue) -> JsonValue {
    match &grant.schema {
        Some(schema) => schema.decode(&row),
        None => row,
    }
}

pub async fn db_get(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::query")?;
    let req: GetRequest = parse_req(payload)?;
    let grant = require_model(ctx, &req.model, false)?;

    // A record the rules do not let the caller read is answered as if it were not there.
    let Some(scope) = guard::scope(ctx, grant, Operation::Read, "r").await? else {
        return Ok(serde_json::json!({ "ok": true, "data": JsonValue::Null }));
    };
    let (hidden, _) = guard::field_limits(ctx, grant).await?;
    let mut binds = record_binds(grant, &req.id);
    binds.extend(scope.binds);
    let body = if scope.sql.is_empty() {
        "LET $rows = SELECT * FROM type::record($__table, $__key);".to_string()
    } else {
        format!("LET $rows = SELECT * FROM type::record($__table, $__key) WHERE {};", scope.sql)
    };
    let rows = run_audited(
        ctx,
        Access { operation: "read", model: req.model.clone(), table: &grant.table },
        &body,
        RECORD_ID,
        binds,
        Extra::default(),
    )
    .await?;
    let mut records: Vec<JsonValue> = rows.into_iter().take(1).map(|row| guard::strip(&hidden, decode(grant, row))).collect();
    if let (Some(schema), false) = (&grant.schema, req.expand.is_empty()) {
        super::nested::expand(ctx, schema, &mut records, &req.expand).await?;
    }
    Ok(serde_json::json!({ "ok": true, "data": records.into_iter().next() }))
}

/// The `WHERE` condition for a filter over `grant`'s model, with the values it binds (the table
/// is always bound as `$__table`), narrowed by what the model's rules let the caller read. The
/// condition is empty when everything matches. `None`: the rules let the caller read nothing.
/// Also answers which fields the caller may not see.
async fn guarded_condition(
    ctx: &PluginHostContext,
    grant: &ModelGrant,
    model: &str,
    filter: &Map<String, JsonValue>,
) -> Result<Option<(String, Vec<(String, JsonValue)>, std::collections::HashSet<String>)>, HostError> {
    let parsed = Filter::parse(filter)?;
    let (hidden, _) = guard::field_limits(ctx, grant).await?;
    guard::check_unreadable(&hidden, parsed.fields(), model)?;
    let compiled = match &grant.schema {
        Some(schema) => compile_filter(&parsed, schema.as_ref(), "f")?,
        None => compile_filter(&parsed, &Unchecked, "f")?,
    };
    let Some(scope) = guard::scope(ctx, grant, Operation::Read, "r").await? else { return Ok(None) };
    let mut binds = vec![("__table".to_string(), JsonValue::String(grant.table.clone()))];
    binds.extend(compiled.binds);
    binds.extend(scope.binds);
    let condition = match (compiled.sql.is_empty(), scope.sql.is_empty()) {
        (true, true) => String::new(),
        (false, true) => compiled.sql,
        (true, false) => scope.sql,
        (false, false) => format!("({}) AND ({})", compiled.sql, scope.sql),
    };
    Ok(Some((condition, binds, hidden)))
}

#[derive(Debug, Deserialize)]
pub struct CountRequest {
    pub model: String,
    #[serde(default)]
    pub filter: Map<String, JsonValue>,
}

#[derive(Debug, Deserialize)]
pub struct AggregateRequest {
    pub model: String,
    #[serde(default)]
    pub filter: Map<String, JsonValue>,
    #[serde(default)]
    pub group_by: Vec<String>,
    pub aggs: Map<String, JsonValue>,
}

/// How many records match a filter, without reading them.
pub async fn db_count(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::query")?;
    let req: CountRequest = parse_req(payload)?;
    let grant = require_model(ctx, &req.model, false)?;
    let Some((condition, binds, _)) = guarded_condition(ctx, grant, &req.model, &req.filter).await? else {
        return Ok(serde_json::json!({ "ok": true, "data": 0 }));
    };
    let mut body = String::from("LET $rows = SELECT count() AS n FROM type::table($__table)");
    if !condition.is_empty() {
        body.push_str(" WHERE ");
        body.push_str(&condition);
    }
    body.push_str(" GROUP ALL;");
    let rows = run_audited(
        ctx,
        Access { operation: "read", model: req.model.clone(), table: &grant.table },
        &body,
        "[]",
        binds,
        Extra::default(),
    )
    .await?;
    // No matching record leaves no group at all.
    let count = rows.first().and_then(|row| row.get("n")).and_then(JsonValue::as_u64).unwrap_or(0);
    Ok(serde_json::json!({ "ok": true, "data": count }))
}

/// Grouped figures over the records that match a filter: counts, sums, averages, minimums and
/// maximums, per value of the `group_by` fields (or one row for the whole set).
pub async fn db_aggregate(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::query")?;
    let req: AggregateRequest = parse_req(payload)?;
    let grant = require_model(ctx, &req.model, false)?;
    let Some((condition, binds, hidden)) = guarded_condition(ctx, grant, &req.model, &req.filter).await? else {
        return Ok(serde_json::json!({ "ok": true, "data": [] }));
    };
    let aggregate = Aggregate::parse(&req.group_by, &req.aggs)?;
    guard::check_unreadable(&hidden, aggregate.fields(), &req.model)?;
    let (select, group) = match &grant.schema {
        Some(schema) => aggregate.compile(schema.as_ref())?,
        None => aggregate.compile(&Unchecked)?,
    };
    let decimals = match &grant.schema {
        Some(schema) => aggregate.decimal_results(schema.as_ref()),
        None => Vec::new(),
    };
    let mut body = format!("LET $rows = SELECT {select} FROM type::table($__table)");
    if !condition.is_empty() {
        body.push_str(" WHERE ");
        body.push_str(&condition);
    }
    body.push_str(&format!(" {group} LIMIT {MAX_ROWS_PER_FIND};"));
    let rows = run_audited(
        ctx,
        Access { operation: "read", model: req.model.clone(), table: &grant.table },
        &body,
        "[]",
        binds,
        Extra::default(),
    )
    .await?;
    let rows: Vec<JsonValue> = rows
        .into_iter()
        .map(|mut row| {
            for (name, scale) in &decimals {
                if let Some(value) = row.get(name) {
                    let shown = crate::data_model::decimal::present(value, *scale);
                    row[name.as_str()] = shown;
                }
            }
            row
        })
        .collect();
    Ok(serde_json::json!({ "ok": true, "data": rows }))
}

pub async fn db_find(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::query")?;
    let req: FindRequest = parse_req(payload)?;
    let mut reply = find_records(ctx, payload).await?;
    if !req.expand.is_empty() {
        let grant = require_model(ctx, &req.model, false)?;
        if let Some(schema) = &grant.schema {
            let mut rows: Vec<JsonValue> = reply.get("data").and_then(JsonValue::as_array).cloned().unwrap_or_default();
            super::nested::expand(ctx, schema, &mut rows, &req.expand).await?;
            reply["data"] = JsonValue::Array(rows);
        }
    }
    Ok(reply)
}

/// `db::find` without the rows of child fields.
pub(super) async fn find_records(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::query")?;
    let req: FindRequest = parse_req(payload)?;
    let grant = require_model(ctx, &req.model, false)?;

    let Some((condition, mut binds, hidden)) = guarded_condition(ctx, grant, &req.model, &req.filter).await? else {
        return Ok(serde_json::json!({ "ok": true, "data": [] }));
    };
    let mut body = String::from("LET $rows = SELECT * FROM type::table($__table)");
    if !condition.is_empty() {
        body.push_str(" WHERE ");
        body.push_str(&condition);
    }
    if let Some(order) = &req.order {
        // A leading `-` sorts the other way (`-rate_date`: newest first).
        let (name, direction) = match order.strip_prefix('-') {
            Some(name) => (name, " DESC"),
            None => (order.as_str(), ""),
        };
        guard::check_unreadable(&hidden, [name], &req.model)?;
        let column = match &grant.schema {
            Some(schema) => schema.column_id(name)?.to_string(),
            None => name.to_string(),
        };
        validate_field_name(&column)?;
        body.push_str(&format!(" ORDER BY {column}{direction}"));
    }
    let limit = req.limit.map_or(MAX_ROWS_PER_FIND, |limit| limit.min(MAX_ROWS_PER_FIND));
    body.push_str(&format!(" LIMIT {limit}"));
    if let Some(offset) = req.offset {
        body.push_str(&format!(" START {offset}"));
    }
    body.push(';');

    let rows = run_audited(
        ctx,
        Access { operation: "read", model: req.model.clone(), table: &grant.table },
        &body,
        "$rows.id",
        binds,
        Extra::default(),
    )
    .await?;
    let rows: Vec<JsonValue> = rows.into_iter().map(|row| guard::strip(&hidden, decode(grant, row))).collect();
    Ok(serde_json::json!({ "ok": true, "data": rows }))
}

pub async fn db_create(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::mutate")?;
    let req: CreateRequest = parse_req(payload)?;
    if let Some(reply) = super::nested::create(ctx, &req).await? {
        return Ok(reply);
    }
    run_plan(ctx, plan_create_with(ctx, req.model, req.data, Options::default()).await?).await
}

async fn plan_create<'a>(ctx: &'a PluginHostContext, payload: &JsonValue) -> Result<Plan<'a>, HostError> {
    let req: CreateRequest = parse_req(payload)?;
    plan_create_with(ctx, req.model, req.data, Options::default()).await
}

pub(super) async fn plan_create_with<'a>(
    ctx: &'a PluginHostContext,
    model: String,
    given: Map<String, JsonValue>,
    options: Options,
) -> Result<Plan<'a>, HostError> {
    ctx.require_cap("db::mutate")?;
    let grant = require_model(ctx, &model, true)?;
    if given.is_empty() && !options.allow_empty {
        return Err(HostError::InvalidPayload("data must not be empty".into()));
    }
    let (_, locked) = guard::field_limits(ctx, grant).await?;
    guard::check_locked(&locked, given.keys(), &model)?;
    let scope = guard::scope(ctx, grant, Operation::Create, "d8").await?.ok_or_else(|| guard::denied(&model, "create"))?;
    // Checked against the model and keyed by field id, or (without one) as given.
    let data = match (&grant.schema, &options.inject) {
        (Some(schema), Some(inject)) => schema.encode_create_row(&given, &inject.field)?,
        (Some(schema), None) => schema.encode_create(&given)?,
        (None, _) => given,
    };
    for key in data.keys() {
        validate_field_name(key)?;
    }

    let mut binds = vec![
        ("__table".to_string(), JsonValue::String(grant.table.clone())),
        ("__data".to_string(), JsonValue::Object(data.clone())),
    ];
    let mut extra = chatter_extra(grant, "create")?;
    let mut content = "$__data".to_string();
    if let Some(schema) = &grant.schema {
        super::graph::create_extra(schema, &data, &mut binds, &mut extra);
        super::graph::link_checks(schema, &data, &mut binds, &mut extra);
        let injected = options.inject.as_ref().map(|i| (i.column.as_str(), i.value_sql.as_str()));
        content = super::integrity::create_content(schema, &data, injected, &mut binds, &mut extra)?;
        super::integrity::derived_fields(schema, &mut extra)?;
        if !options.defer_checks {
            super::integrity::model_checks(schema, "$rows", &mut binds, &mut extra)?;
        }
        if !options.skip_parent_refresh {
            super::integrity::refresh_parents(schema, &mut extra)?;
        }
    }
    if let (Some((field, states)), Some(schema)) = (guard::workflow_initial(ctx, grant).await?, &grant.schema) {
        let column = schema.column_id(&field)?;
        validate_field_name(column)?;
        extra.after.push(format!(
            "IF array::len((SELECT VALUE id FROM $rows[0].id WHERE {column} IN $d30010)) = 0 {{ THROW $d30011; }};"
        ));
        binds.push(("d30011".to_string(), JsonValue::String(format!("aether-denied: a new `{model}` record starts as {}", states.join(" or ")))));
        binds.push(("d30010".to_string(), JsonValue::Array(states.into_iter().map(JsonValue::String).collect())));
    }
    if !scope.sql.is_empty() {
        // The new record must be one the rules let this person create; if not, the whole write is undone.
        extra.after.push(format!(
            "IF array::len((SELECT VALUE id FROM $rows[0].id WHERE {})) = 0 {{ THROW 'aether-denied: you may not create `{}` records like this'; }};",
            scope.sql, model
        ));
        binds.extend(scope.binds);
    }
    Ok(Plan {
        access: Access { operation: "create", model: model, table: &grant.table },
        grant,
        body: format!("LET $rows = (CREATE type::table($__table) CONTENT {content} RETURN AFTER);"),
        ids: "$rows.id".into(),
        binds,
        extra,
    })
}

pub async fn db_update(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::mutate")?;
    let req: UpdateRequest = parse_req(payload)?;
    if let Some(reply) = super::nested::update(ctx, &req).await? {
        return Ok(reply);
    }
    run_plan(ctx, plan_update_with(ctx, req.model, req.id, req.data, Options::default()).await?).await
}

async fn plan_update<'a>(ctx: &'a PluginHostContext, payload: &JsonValue) -> Result<Plan<'a>, HostError> {
    let req: UpdateRequest = parse_req(payload)?;
    plan_update_with(ctx, req.model, req.id, req.data, Options::default()).await
}

pub(super) async fn plan_update_with<'a>(
    ctx: &'a PluginHostContext,
    model: String,
    id: String,
    data: Map<String, JsonValue>,
    options: Options,
) -> Result<Plan<'a>, HostError> {
    ctx.require_cap("db::mutate")?;
    let grant = require_model(ctx, &model, true)?;
    let (_, locked) = guard::field_limits(ctx, grant).await?;
    guard::check_locked(&locked, data.keys(), &model)?;
    let scope = guard::scope(ctx, grant, Operation::Write, "d8").await?.ok_or_else(|| guard::denied(&model, "change"))?;
    let mut binds = record_binds(grant, &id);
    let mut extra = chatter_extra(grant, "update")?;
    add_record_check(&mut extra, &mut binds, scope, &model, "change");
    let body = match &grant.schema {
        Some(schema) => {
            // One statement that sets the changed columns (by id) and clears the ones set to null.
            let (set, clear) = schema.encode_update(&data)?;
            workflow_move(ctx, grant, schema, &set, &clear, &model, &mut binds, &mut extra).await?;
            super::graph::update_extra(schema, &set, &clear, &mut binds, &mut extra);
            super::graph::link_checks(schema, &set, &mut binds, &mut extra);
            super::integrity::derived_fields(schema, &mut extra)?;
            if !options.defer_checks {
                super::integrity::model_checks(schema, "$rows", &mut binds, &mut extra)?;
            }
            if !options.skip_parent_refresh {
                super::integrity::refresh_parents(schema, &mut extra)?;
            }
            let mut assignments = Vec::new();
            for (index, (column, value)) in set.into_iter().enumerate() {
                validate_field_name(&column)?;
                assignments.push(format!("{column} = $d{index}"));
                binds.push((format!("d{index}"), value));
            }
            for column in &clear {
                validate_field_name(column)?;
                assignments.push(format!("{column} = NONE"));
            }
            let clause = if assignments.is_empty() {
                String::new()
            } else {
                format!(" SET {}", assignments.join(", "))
            };
            format!("LET $rows = (UPDATE type::record($__table, $__key){clause} RETURN AFTER);")
        }
        None => {
            for key in data.keys() {
                validate_field_name(key)?;
            }
            binds.push(("__data".to_string(), JsonValue::Object(data)));
            "LET $rows = (UPDATE type::record($__table, $__key) MERGE $__data RETURN AFTER);".to_string()
        }
    };
    Ok(Plan {
        access: Access { operation: "update", model: model, table: &grant.table },
        grant,
        body,
        ids: RECORD_ID.into(),
        binds,
        extra,
    })
}

/// A write that sets the workflow field is a move: refused unless a transition the caller may make
/// leads to the new state from the record's state as it is now (checked in the write's own
/// transaction, before the change).
#[allow(clippy::too_many_arguments)]
async fn workflow_move(
    ctx: &PluginHostContext,
    grant: &ModelGrant,
    schema: &crate::data_model::ModelSchema,
    set: &Map<String, JsonValue>,
    clear: &[String],
    model: &str,
    binds: &mut Vec<(String, JsonValue)>,
    extra: &mut Extra,
) -> Result<(), HostError> {
    let Some(workflow) = grant.rules.as_ref().and_then(|rules| rules.workflow.as_ref()) else { return Ok(()) };
    let column = schema.column_id(&workflow.field)?.to_string();
    let refuse = |to: &str| HostError::Denied(format!("you may not move this `{model}` record to `{to}`"));
    if clear.contains(&column) {
        return match guard::workflow_scope(ctx, grant, "", "d9").await? {
            Some(scope) if scope.sql.is_empty() => Ok(()),
            _ => Err(refuse("(nothing)")),
        };
    }
    let Some(to) = set.get(&column).and_then(JsonValue::as_str) else { return Ok(()) };
    let Some(scope) = guard::workflow_scope(ctx, grant, to, "d9").await? else { return Err(refuse(to)) };
    if scope.sql.is_empty() {
        return Ok(());
    }
    extra.before.push(format!(
        "IF array::len((SELECT VALUE id FROM type::record($__table, $__key) WHERE {})) = 0 {{ THROW $d30012; }};",
        scope.sql
    ));
    binds.extend(scope.binds);
    binds.push(("d30012".to_string(), JsonValue::String(format!("aether-denied: you may not move this `{model}` record to `{to}` from its state now"))));
    Ok(())
}

/// Add to a number field in place. The addition happens in the database, in one statement, so two
/// callers incrementing at once both count: use it for counters and sequences, never read, add and
/// write back.
pub async fn db_increment(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    run_plan(ctx, plan_increment(ctx, payload).await?).await
}

async fn plan_increment<'a>(ctx: &'a PluginHostContext, payload: &JsonValue) -> Result<Plan<'a>, HostError> {
    ctx.require_cap("db::mutate")?;
    let req: IncrementRequest = parse_req(payload)?;
    let grant = require_model(ctx, &req.model, true)?;
    let (column, by) = match &grant.schema {
        Some(schema) => (schema.numeric_column(&req.field)?.to_string(), schema.increment_by(&req.field, &req.by)?),
        None => (req.field.clone(), req.by.clone()),
    };
    let number = by.as_f64().filter(|n| n.is_finite());
    let Some(number) = number else {
        return Err(HostError::InvalidPayload("`by` must be a finite number".into()));
    };
    validate_field_name(&column)?;
    let (_, locked) = guard::field_limits(ctx, grant).await?;
    guard::check_locked(&locked, [&req.field], &req.model)?;
    let scope = guard::scope(ctx, grant, Operation::Write, "d8").await?.ok_or_else(|| guard::denied(&req.model, "change"))?;
    let mut binds = record_binds(grant, &req.id);
    // A whole `by` stays a whole number in the database.
    let by = if number.fract() == 0.0 && number.abs() < 9e15 {
        JsonValue::from(number as i64)
    } else {
        JsonValue::from(number)
    };
    binds.push(("__by".to_string(), by));
    let body = format!(
        "LET $rows = (UPDATE type::record($__table, $__key) SET {column} = ({column} ?? 0) + $__by RETURN AFTER);"
    );
    let mut extra = chatter_extra(grant, "update")?;
    add_record_check(&mut extra, &mut binds, scope, &req.model, "change");
    Ok(Plan {
        access: Access { operation: "update", model: req.model, table: &grant.table },
        grant,
        body,
        ids: RECORD_ID.into(),
        binds,
        extra,
    })
}

pub async fn db_delete(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::mutate")?;
    let req: DeleteRequest = parse_req(payload)?;
    if let Some(reply) = super::nested::delete(ctx, &req).await? {
        return Ok(reply);
    }
    run_plan(ctx, plan_delete_with(ctx, req.model, req.id, Options::default()).await?).await
}

async fn plan_delete<'a>(ctx: &'a PluginHostContext, payload: &JsonValue) -> Result<Plan<'a>, HostError> {
    let req: DeleteRequest = parse_req(payload)?;
    plan_delete_with(ctx, req.model, req.id, Options::default()).await
}

pub(super) async fn plan_delete_with<'a>(
    ctx: &'a PluginHostContext,
    model: String,
    id: String,
    options: Options,
) -> Result<Plan<'a>, HostError> {
    ctx.require_cap("db::mutate")?;
    let grant = require_model(ctx, &model, true)?;
    let scope = guard::scope(ctx, grant, Operation::Delete, "d8").await?.ok_or_else(|| guard::denied(&model, "delete"))?;
    let mut binds = record_binds(grant, &id);
    let mut extra = chatter_extra(grant, "delete")?;
    if let Some(schema) = &grant.schema {
        super::graph::delete_extra(schema, &mut extra);
        if !options.skip_parent_refresh {
            super::integrity::refresh_parents(schema, &mut extra)?;
        }
    }
    add_record_check(&mut extra, &mut binds, scope, &model, "delete");
    Ok(Plan {
        access: Access { operation: "delete", model: model, table: &grant.table },
        grant,
        body: "LET $rows = (DELETE type::record($__table, $__key) RETURN BEFORE);".into(),
        ids: RECORD_ID.into(),
        binds,
        extra,
    })
}

/// Most writes one `db::transaction` may hold.
pub const MAX_TRANSACTION_OPS: usize = 50;

/// Variables each write declares for itself. In a transaction every write gets its own copy
/// (`$rows_0`, `$rows_1`, …) so one cannot overwrite another's.
const PER_WRITE_VARIABLES: &[&str] =
    &["rows", "__table", "__key", "__data", "__by", "__before", "__changed", "__audit_model", "__audit_table", "__audit_operation"];

fn is_per_write(name: &str) -> bool {
    PER_WRITE_VARIABLES.contains(&name)
        || (name.len() > 1 && name.starts_with('d') && name[1..].bytes().all(|byte| byte.is_ascii_digit()))
}

/// `text` with every per-write variable (`$rows`, `$__table`, `$d0`, …) renamed by `suffix`.
fn rename_variables(text: &str, suffix: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut rest = text;
    while let Some(dollar) = rest.find('$') {
        out.push_str(&rest[..=dollar]);
        rest = &rest[dollar + 1..];
        let end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(rest.len());
        let (name, tail) = rest.split_at(end);
        out.push_str(name);
        if is_per_write(name) {
            out.push_str(suffix);
        }
        rest = tail;
    }
    out.push_str(rest);
    out
}

/// Several writes, applied together or not at all. Payload:
/// `{ "ops": [ { "op": "create" | "update" | "delete" | "increment", "model": …, … }, … ] }`
/// where each entry has the fields the single command takes. Every write is checked against the
/// plugin's capabilities and model grants exactly as when sent alone, and each leaves its own
/// audit row (and chatter entries), all inside the one transaction. The answer's `data` lists
/// each write's record, in order.
pub async fn db_transaction(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::transaction")?;
    let ops = payload
        .get("ops")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| HostError::InvalidPayload("`ops` must be a list of writes".into()))?;
    if ops.is_empty() || ops.len() > MAX_TRANSACTION_OPS {
        return Err(HostError::InvalidPayload(format!(
            "a transaction holds 1 to {MAX_TRANSACTION_OPS} writes, not {}",
            ops.len()
        )));
    }

    let mut plans = Vec::with_capacity(ops.len());
    for (index, op) in ops.iter().enumerate() {
        let name = op.get("op").and_then(JsonValue::as_str).unwrap_or_default();
        let plan = match name {
            "create" => plan_create(ctx, op).await,
            "update" => plan_update(ctx, op).await,
            "delete" => plan_delete(ctx, op).await,
            "increment" => plan_increment(ctx, op).await,
            other => Err(HostError::InvalidPayload(format!(
                "write {index}: `op` must be create, update, delete or increment, not `{other}`"
            ))),
        }
        .map_err(|error| match error {
            HostError::InvalidPayload(message) => HostError::InvalidPayload(format!("write {index}: {message}")),
            other => other,
        })?;
        plans.push(plan);
    }

    let data = run_composed(ctx, &plans).await?;
    Ok(serde_json::json!({ "ok": true, "data": data }))
}

/// Run several writes in one transaction, each with its own copy of the per-write variables, and
/// answer each write's record (or null), decoded as the plugin sees it.
pub(super) async fn run_composed(ctx: &PluginHostContext, plans: &[Plan<'_>]) -> Result<Vec<JsonValue>, HostError> {
    let mut hiddens = Vec::with_capacity(plans.len());
    for plan in plans {
        hiddens.push(guard::field_limits(ctx, plan.grant).await?.0);
    }
    let mut statements = String::from("BEGIN TRANSACTION;\n");
    let mut binds: Vec<(String, JsonValue)> = Vec::new();
    let mut statement_count = 1; // BEGIN
    let mut returned = Vec::with_capacity(plans.len());
    for (index, plan) in plans.iter().enumerate() {
        let suffix = format!("_{index}");
        let before = rename_variables(&plan.extra.before.join("\n"), &suffix);
        let after = rename_variables(&plan.extra.after.join("\n"), &suffix);
        let body = rename_variables(&plan.body, &suffix);
        let ids = rename_variables(&plan.ids, &suffix);
        statements.push_str(&format!(
            "{before}\n{body}\n\
             CREATE data_access SET request_id = $__audit_request, actor_type = $__audit_actor_type, \
             actor_id = $__audit_actor_id, plugin = $__audit_plugin, function_name = $__audit_function, \
             model = $__audit_model{suffix}, table_name = $__audit_table{suffix}, \
             operation = $__audit_operation{suffix}, record_ids = {ids}, \
             record_count = array::len($rows{suffix}), ip = $__audit_ip;\n{after}\n"
        ));
        statement_count += plan.extra.before.len() + 2 + plan.extra.after.len();
        returned.push(format!("$rows{suffix}"));
        for (name, value) in &plan.binds {
            let name = if is_per_write(name) { format!("{name}{suffix}") } else { name.clone() };
            binds.push((name, value.clone()));
        }
        binds.push((format!("__audit_model{suffix}"), JsonValue::String(plan.access.model.clone())));
        binds.push((format!("__audit_table{suffix}"), JsonValue::String(plan.access.table.to_string())));
        binds.push((format!("__audit_operation{suffix}"), JsonValue::String(plan.access.operation.to_string())));
    }
    statements.push_str(&format!("RETURN [{}];\nCOMMIT TRANSACTION;", returned.join(", ")));

    let mut query = ctx
        .db
        .query(statements)
        .bind(("__audit_request", ctx.audit.request_id.clone()))
        .bind(("__audit_actor_type", ctx.audit.actor.kind().to_string()))
        .bind(("__audit_actor_id", ctx.audit.actor.id().map(str::to_string)))
        .bind(("__audit_plugin", ctx.plugin_name.clone()))
        .bind(("__audit_function", ctx.function.clone()))
        .bind(("__audit_ip", ctx.audit.ip.clone()))
        .bind(("__cm_author", ctx.audit.actor.id().unwrap_or("system").to_string()));
    for (name, value) in binds {
        query = query.bind((name, value));
    }
    let mut response = checked(query.await.map_err(map_db_error)?)?;
    let results: Vec<Vec<JsonValue>> = response.take(statement_count)?;
    let data: Vec<JsonValue> = results
        .into_iter()
        .zip(plans)
        .zip(&hiddens)
        .map(|((rows, plan), hidden)| rows.into_iter().next().map_or(JsonValue::Null, |row| guard::strip(hidden, decode(plan.grant, row))))
        .collect();
    Ok(data)
}

pub(super) fn strip_table_prefix<'a>(id: &'a str, table: &str) -> &'a str {
    id.strip_prefix(&format!("{table}:")).unwrap_or(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_write_in_a_transaction_gets_its_own_variables() {
        let text = "LET $rows = (UPDATE type::record($__table, $__key) SET a = $d0, b = $d12 RETURN AFTER); \
                    $rows[0].id; $__cm_author; $__audit_plugin; |$k| $__before[$k]; $d; $data";
        let renamed = rename_variables(text, "_3");
        assert!(renamed.contains("LET $rows_3 = (UPDATE type::record($__table_3, $__key_3) SET a = $d0_3, b = $d12_3"));
        assert!(renamed.contains("$rows_3[0].id"));
        assert!(renamed.contains("$__before_3[$k]"), "lambda variables are not renamed");
        for shared in ["$__cm_author;", "$__audit_plugin;", "$d;", "$data"] {
            assert!(renamed.contains(shared), "{shared} must stay as it is: {renamed}");
        }
    }

    #[test]
    fn field_and_table_names_must_be_plain_identifiers() {
        for good in ["body", "created_at", "chat_message", "A1"] {
            assert!(validate_ident(good).is_ok(), "{good}");
        }
        for bad in ["", "a b", "a;b", "a.b", "a-b", "x) OR true --", "type::table"] {
            assert!(validate_ident(bad).is_err(), "{bad}");
        }
    }
}
