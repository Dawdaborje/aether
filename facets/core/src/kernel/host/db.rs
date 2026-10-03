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
    "data_access",
    "group_roles",
    "groups",
    "installed_plugin_depends_on",
    "installed_plugins",
    "invitations",
    "media",
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
    "schema_migrations",
    "settings_group_items",
    "settings_groups",
    "settings_items",
    "storage_backends",
    "ui_theme_config",
    "ui_themes",
    "visitors",
];

/// Index of the `RETURN $rows` statement in the audited transaction:
/// `BEGIN`, the work, the audit insert, then the return.
const RETURN_INDEX: usize = 3;

struct Access<'a> {
    operation: &'static str,
    model: &'a str,
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
) -> Result<Vec<JsonValue>, HostError> {
    let surql = format!(
        r#"
        BEGIN TRANSACTION;
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
        .bind(("__audit_model", access.model.to_string()))
        .bind(("__audit_table", access.table.to_string()))
        .bind(("__audit_operation", access.operation.to_string()))
        .bind(("__audit_ip", ctx.audit.ip.clone()));
    for (name, value) in binds {
        query = query.bind((name, value));
    }
    let mut response = query.await?.check()?;
    Ok(response.take(RETURN_INDEX)?)
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

pub async fn db_get(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::query")?;
    let req: GetRequest = parse_req(payload)?;
    let grant = require_model(ctx, &req.model, false)?;

    let rows = run_audited(
        ctx,
        Access { operation: "read", model: &req.model, table: &grant.table },
        "LET $rows = SELECT * FROM type::record($__table, $__key);",
        RECORD_ID,
        record_binds(grant, &req.id),
    )
    .await?;
    Ok(serde_json::json!({ "ok": true, "data": rows.into_iter().next() }))
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
        validate_field_name(field)?;
        let placeholder = format!("f{i}");
        where_parts.push(format!("{field} = ${placeholder}"));
        binds.push((placeholder, value.clone()));
    }

    let mut body = String::from("LET $rows = SELECT * FROM type::table($__table)");
    if !where_parts.is_empty() {
        body.push_str(" WHERE ");
        body.push_str(&where_parts.join(" AND "));
    }
    if let Some(order) = &req.order {
        validate_field_name(order)?;
        body.push_str(&format!(" ORDER BY {order}"));
    }
    let limit = req.limit.map_or(MAX_ROWS_PER_FIND, |limit| limit.min(MAX_ROWS_PER_FIND));
    body.push_str(&format!(" LIMIT {limit}"));
    if let Some(offset) = req.offset {
        body.push_str(&format!(" START {offset}"));
    }
    body.push(';');

    let rows = run_audited(
        ctx,
        Access { operation: "read", model: &req.model, table: &grant.table },
        &body,
        "$rows.id",
        binds,
    )
    .await?;
    Ok(serde_json::json!({ "ok": true, "data": rows }))
}

pub async fn db_create(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::mutate")?;
    let req: CreateRequest = parse_req(payload)?;
    let grant = require_model(ctx, &req.model, true)?;
    if req.data.is_empty() {
        return Err(HostError::InvalidPayload("data must not be empty".into()));
    }
    for key in req.data.keys() {
        validate_field_name(key)?;
    }

    let binds = vec![
        ("__table".to_string(), JsonValue::String(grant.table.clone())),
        ("__data".to_string(), JsonValue::Object(req.data)),
    ];
    let rows = run_audited(
        ctx,
        Access { operation: "create", model: &req.model, table: &grant.table },
        "LET $rows = (CREATE type::table($__table) CONTENT $__data RETURN AFTER);",
        "$rows.id",
        binds,
    )
    .await?;
    Ok(serde_json::json!({ "ok": true, "data": rows.into_iter().next() }))
}

pub async fn db_update(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::mutate")?;
    let req: UpdateRequest = parse_req(payload)?;
    let grant = require_model(ctx, &req.model, true)?;
    for key in req.data.keys() {
        validate_field_name(key)?;
    }

    let mut binds = record_binds(grant, &req.id);
    binds.push(("__data".to_string(), JsonValue::Object(req.data)));
    let rows = run_audited(
        ctx,
        Access { operation: "update", model: &req.model, table: &grant.table },
        "LET $rows = (UPDATE type::record($__table, $__key) MERGE $__data RETURN AFTER);",
        RECORD_ID,
        binds,
    )
    .await?;
    Ok(serde_json::json!({ "ok": true, "data": rows.into_iter().next() }))
}

pub async fn db_delete(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::mutate")?;
    let req: DeleteRequest = parse_req(payload)?;
    let grant = require_model(ctx, &req.model, true)?;

    let rows = run_audited(
        ctx,
        Access { operation: "delete", model: &req.model, table: &grant.table },
        "LET $rows = (DELETE type::record($__table, $__key) RETURN BEFORE);",
        RECORD_ID,
        record_binds(grant, &req.id),
    )
    .await?;
    Ok(serde_json::json!({ "ok": true, "data": rows.into_iter().next() }))
}

fn strip_table_prefix<'a>(id: &'a str, table: &str) -> &'a str {
    id.strip_prefix(&format!("{table}:")).unwrap_or(id)
}

#[cfg(test)]
mod tests {
    use super::*;

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
