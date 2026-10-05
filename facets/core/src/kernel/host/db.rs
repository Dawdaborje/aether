use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as JsonValue};

use super::context::{ModelGrant, PluginHostContext};
use super::error::HostError;

#[derive(Debug, Deserialize)]
pub struct GetRequest {
    pub model: String,
    pub id: String,
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
    /// What to add; negative subtracts. Defaults to one.
    #[serde(default = "one")]
    pub by: f64,
}

fn one() -> f64 {
    1.0
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

fn require_model<'a>(
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

fn validate_ident(name: &str) -> Result<(), HostError> {
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

fn parse_req<T: serde::de::DeserializeOwned>(payload: &JsonValue) -> Result<T, HostError> {
    serde_json::from_value(payload.clone())
        .map_err(|e| HostError::InvalidPayload(e.to_string()))
}

/// Rows a single `db::find` may return, whatever limit the plugin asks for.
pub const MAX_ROWS_PER_FIND: u32 = 1000;

/// Tables the kernel owns in every organization database. A plugin cannot map
/// a model onto one of them.
const RESERVED_TABLES: &[&str] = &[
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
struct Extra {
    before: Vec<String>,
    after: Vec<String>,
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

struct Access<'a> {
    operation: &'static str,
    model: String,
    table: &'a str,
}

/// Run `body` (which must `LET $rows = …;`) and record the access in
/// `data_access` inside one transaction. If the audit row cannot be written,
/// nothing is applied and the call fails. `ids` is the SurrealQL expression for
/// the record ids touched.
async fn run_audited(
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
    let mut response = query.await?.check()?;
    Ok(response.take(extra.return_index())?)
}

fn record_binds(grant: &ModelGrant, id: &str) -> Vec<(String, JsonValue)> {
    vec![
        ("__table".to_string(), JsonValue::String(grant.table.clone())),
        (
            "__key".to_string(),
            JsonValue::String(strip_table_prefix(id, &grant.table).to_string()),
        ),
    ]
}

const RECORD_ID: &str = "[type::record($__table, $__key)]";

/// One write, ready to run: the statement, what it binds, and the audit facts about it. A single
/// command runs one plan in its own transaction; `db::transaction` runs several in one.
struct Plan<'a> {
    access: Access<'a>,
    grant: &'a ModelGrant,
    body: String,
    ids: String,
    binds: Vec<(String, JsonValue)>,
    extra: Extra,
}

async fn run_plan(ctx: &PluginHostContext, plan: Plan<'_>) -> Result<JsonValue, HostError> {
    let grant = plan.grant;
    let rows = run_audited(ctx, plan.access, &plan.body, &plan.ids, plan.binds, plan.extra).await?;
    Ok(serde_json::json!({ "ok": true, "data": rows.into_iter().next().map(|row| decode(grant, row)) }))
}

/// A record as the plugin sees it: field names, not the ids it is stored under.
fn decode(grant: &ModelGrant, row: JsonValue) -> JsonValue {
    match &grant.schema {
        Some(schema) => schema.decode(&row),
        None => row,
    }
}

pub async fn db_get(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::query")?;
    let req: GetRequest = parse_req(payload)?;
    let grant = require_model(ctx, &req.model, false)?;

    let rows = run_audited(
        ctx,
        Access { operation: "read", model: req.model.clone(), table: &grant.table },
        "LET $rows = SELECT * FROM type::record($__table, $__key);",
        RECORD_ID,
        record_binds(grant, &req.id),
        Extra::default(),
    )
    .await?;
    Ok(serde_json::json!({ "ok": true, "data": rows.into_iter().next().map(|row| decode(grant, row)) }))
}

pub async fn db_find(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::query")?;
    let req: FindRequest = parse_req(payload)?;
    let grant = require_model(ctx, &req.model, false)?;

    let mut where_parts = Vec::new();
    let mut binds: Vec<(String, JsonValue)> = vec![(
        "__table".to_string(),
        JsonValue::String(grant.table.clone()),
    )];
    for (i, (field, value)) in req.filter.iter().enumerate() {
        // With a model definition the field is looked up by name and stored under its id.
        let (column, value) = match &grant.schema {
            Some(schema) => (schema.column_id(field)?.to_string(), schema.filter_value(field, value)?),
            None => (field.clone(), value.clone()),
        };
        validate_field_name(&column)?;
        let placeholder = format!("f{i}");
        where_parts.push(format!("{column} = ${placeholder}"));
        binds.push((placeholder, value));
    }

    let mut body = String::from("LET $rows = SELECT * FROM type::table($__table)");
    if !where_parts.is_empty() {
        body.push_str(" WHERE ");
        body.push_str(&where_parts.join(" AND "));
    }
    if let Some(order) = &req.order {
        // A leading `-` sorts the other way (`-rate_date`: newest first).
        let (name, direction) = match order.strip_prefix('-') {
            Some(name) => (name, " DESC"),
            None => (order.as_str(), ""),
        };
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
    let rows: Vec<JsonValue> = rows.into_iter().map(|row| decode(grant, row)).collect();
    Ok(serde_json::json!({ "ok": true, "data": rows }))
}

pub async fn db_create(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    run_plan(ctx, plan_create(ctx, payload)?).await
}

fn plan_create<'a>(ctx: &'a PluginHostContext, payload: &JsonValue) -> Result<Plan<'a>, HostError> {
    ctx.require_cap("db::mutate")?;
    let req: CreateRequest = parse_req(payload)?;
    let grant = require_model(ctx, &req.model, true)?;
    if req.data.is_empty() {
        return Err(HostError::InvalidPayload("data must not be empty".into()));
    }
    // Checked against the model and keyed by field id, or (without one) as given.
    let data = match &grant.schema {
        Some(schema) => schema.encode_create(&req.data)?,
        None => req.data,
    };
    for key in data.keys() {
        validate_field_name(key)?;
    }

    let binds = vec![
        ("__table".to_string(), JsonValue::String(grant.table.clone())),
        ("__data".to_string(), JsonValue::Object(data)),
    ];
    Ok(Plan {
        access: Access { operation: "create", model: req.model, table: &grant.table },
        grant,
        body: "LET $rows = (CREATE type::table($__table) CONTENT $__data RETURN AFTER);".into(),
        ids: "$rows.id".into(),
        binds,
        extra: chatter_extra(grant, "create")?,
    })
}

pub async fn db_update(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    run_plan(ctx, plan_update(ctx, payload)?).await
}

fn plan_update<'a>(ctx: &'a PluginHostContext, payload: &JsonValue) -> Result<Plan<'a>, HostError> {
    ctx.require_cap("db::mutate")?;
    let req: UpdateRequest = parse_req(payload)?;
    let grant = require_model(ctx, &req.model, true)?;
    let mut binds = record_binds(grant, &req.id);
    let body = match &grant.schema {
        Some(schema) => {
            // One statement that sets the changed columns (by id) and clears the ones set to null.
            let (set, clear) = schema.encode_update(&req.data)?;
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
            for key in req.data.keys() {
                validate_field_name(key)?;
            }
            binds.push(("__data".to_string(), JsonValue::Object(req.data)));
            "LET $rows = (UPDATE type::record($__table, $__key) MERGE $__data RETURN AFTER);".to_string()
        }
    };
    Ok(Plan {
        access: Access { operation: "update", model: req.model, table: &grant.table },
        grant,
        body,
        ids: RECORD_ID.into(),
        binds,
        extra: chatter_extra(grant, "update")?,
    })
}

/// Add to a number field in place. The addition happens in the database, in one statement, so two
/// callers incrementing at once both count: use it for counters and sequences, never read, add and
/// write back.
pub async fn db_increment(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    run_plan(ctx, plan_increment(ctx, payload)?).await
}

fn plan_increment<'a>(ctx: &'a PluginHostContext, payload: &JsonValue) -> Result<Plan<'a>, HostError> {
    ctx.require_cap("db::mutate")?;
    let req: IncrementRequest = parse_req(payload)?;
    if !req.by.is_finite() {
        return Err(HostError::InvalidPayload("`by` must be a finite number".into()));
    }
    let grant = require_model(ctx, &req.model, true)?;
    let column = match &grant.schema {
        Some(schema) => schema.numeric_column(&req.field)?.to_string(),
        None => req.field.clone(),
    };
    validate_field_name(&column)?;
    let mut binds = record_binds(grant, &req.id);
    // A whole `by` stays a whole number in the database.
    let by = if req.by.fract() == 0.0 && req.by.abs() < 9e15 {
        JsonValue::from(req.by as i64)
    } else {
        JsonValue::from(req.by)
    };
    binds.push(("__by".to_string(), by));
    let body = format!(
        "LET $rows = (UPDATE type::record($__table, $__key) SET {column} = ({column} ?? 0) + $__by RETURN AFTER);"
    );
    Ok(Plan {
        access: Access { operation: "update", model: req.model, table: &grant.table },
        grant,
        body,
        ids: RECORD_ID.into(),
        binds,
        extra: chatter_extra(grant, "update")?,
    })
}

pub async fn db_delete(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    run_plan(ctx, plan_delete(ctx, payload)?).await
}

fn plan_delete<'a>(ctx: &'a PluginHostContext, payload: &JsonValue) -> Result<Plan<'a>, HostError> {
    ctx.require_cap("db::mutate")?;
    let req: DeleteRequest = parse_req(payload)?;
    let grant = require_model(ctx, &req.model, true)?;
    Ok(Plan {
        access: Access { operation: "delete", model: req.model, table: &grant.table },
        grant,
        body: "LET $rows = (DELETE type::record($__table, $__key) RETURN BEFORE);".into(),
        ids: RECORD_ID.into(),
        binds: record_binds(grant, &req.id),
        extra: chatter_extra(grant, "delete")?,
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
            "create" => plan_create(ctx, op),
            "update" => plan_update(ctx, op),
            "delete" => plan_delete(ctx, op),
            "increment" => plan_increment(ctx, op),
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
    let mut response = query.await?.check()?;
    let results: Vec<Vec<JsonValue>> = response.take(statement_count)?;
    let data: Vec<JsonValue> = results
        .into_iter()
        .zip(&plans)
        .map(|(rows, plan)| rows.into_iter().next().map_or(JsonValue::Null, |row| decode(plan.grant, row)))
        .collect();
    Ok(serde_json::json!({ "ok": true, "data": data }))
}

fn strip_table_prefix<'a>(id: &'a str, table: &str) -> &'a str {
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
