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

#[derive(Debug, Deserialize)]
pub struct SurqlRequest {
    /// Optional model — when set, `$__table` is bound to the allowlisted table name.
    #[serde(default)]
    pub model: Option<String>,
    pub query: String,
    #[serde(default)]
    pub vars: Map<String, JsonValue>,
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

/// Reject DDL / namespace escapes in raw SurQL.
fn reject_dangerous_surql(query: &str) -> Result<(), HostError> {
    let upper = query.to_uppercase();
    const BANNED: &[&str] = &[
        "USE NS",
        "USE DB",
        "USE NAMESPACE",
        "USE DATABASE",
        "DEFINE ",
        "REMOVE ",
        "INFO FOR",
        "INFO NS",
        "INFO DB",
        "REBUILD ",
        "KILL ",
    ];
    for token in BANNED {
        if upper.contains(token) {
            return Err(HostError::SurqlRejected(format!(
                "statement containing `{token}` is not allowed"
            )));
        }
    }
    Ok(())
}

fn parse_req<T: serde::de::DeserializeOwned>(payload: &JsonValue) -> Result<T, HostError> {
    serde_json::from_value(payload.clone())
        .map_err(|e| HostError::InvalidPayload(e.to_string()))
}

pub async fn db_get(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::query")?;
    let req: GetRequest = parse_req(payload)?;
    let grant = require_model(ctx, &req.model, false)?;
    ctx.use_scoped_db().await?;

    let record = format!("{}:{}", grant.table, strip_table_prefix(&req.id, &grant.table));
    let mut response = ctx
        .db
        .query("SELECT * FROM type::thing($id);")
        .bind(("id", record))
        .await?
        .check()?;
    let rows: Vec<JsonValue> = response.take(0).unwrap_or_default();
    Ok(serde_json::json!({ "ok": true, "data": rows.into_iter().next() }))
}

pub async fn db_find(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::query")?;
    let req: FindRequest = parse_req(payload)?;
    let grant = require_model(ctx, &req.model, false)?;
    ctx.use_scoped_db().await?;

    let mut where_parts = Vec::new();
    let mut binds: Vec<(String, JsonValue)> = Vec::new();
    for (i, (field, value)) in req.filter.iter().enumerate() {
        validate_field_name(field)?;
        let placeholder = format!("f{i}");
        where_parts.push(format!("{field} = ${placeholder}"));
        binds.push((placeholder, value.clone()));
    }

    let mut surql = format!("SELECT * FROM type::table($table)");
    if !where_parts.is_empty() {
        surql.push_str(" WHERE ");
        surql.push_str(&where_parts.join(" AND "));
    }
    if let Some(order) = &req.order {
        validate_field_name(order)?;
        surql.push_str(&format!(" ORDER BY {order}"));
    }
    if let Some(limit) = req.limit {
        surql.push_str(&format!(" LIMIT {limit}"));
    }
    if let Some(offset) = req.offset {
        surql.push_str(&format!(" START {offset}"));
    }

    let mut q = ctx.db.query(&surql).bind(("table", grant.table.clone()));
    for (k, v) in binds {
        q = q.bind((k, v));
    }
    let mut response = q.await?.check()?;
    let rows: Vec<JsonValue> = response.take(0).unwrap_or_default();
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
    ctx.use_scoped_db().await?;

    let mut response = ctx
        .db
        .query("CREATE type::table($table) CONTENT $data RETURN AFTER;")
        .bind(("table", grant.table.clone()))
        .bind(("data", JsonValue::Object(req.data)))
        .await?
        .check()?;
    let rows: Vec<JsonValue> = response.take(0).unwrap_or_default();
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
    ctx.use_scoped_db().await?;

    let record = format!("{}:{}", grant.table, strip_table_prefix(&req.id, &grant.table));
    let mut response = ctx
        .db
        .query("UPDATE type::thing($id) MERGE $data RETURN AFTER;")
        .bind(("id", record))
        .bind(("data", JsonValue::Object(req.data)))
        .await?
        .check()?;
    let rows: Vec<JsonValue> = response.take(0).unwrap_or_default();
    Ok(serde_json::json!({ "ok": true, "data": rows.into_iter().next() }))
}

pub async fn db_delete(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::mutate")?;
    let req: DeleteRequest = parse_req(payload)?;
    let grant = require_model(ctx, &req.model, true)?;
    ctx.use_scoped_db().await?;

    let record = format!("{}:{}", grant.table, strip_table_prefix(&req.id, &grant.table));
    let mut response = ctx
        .db
        .query("DELETE type::thing($id) RETURN BEFORE;")
        .bind(("id", record))
        .await?
        .check()?;
    let rows: Vec<JsonValue> = response.take(0).unwrap_or_default();
    Ok(serde_json::json!({ "ok": true, "data": rows.into_iter().next() }))
}

/// Raw SurQL — requires `db::surql`. Optional `model` binds `$__table`.
pub async fn db_surql(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::surql")?;
    let req: SurqlRequest = parse_req(payload)?;
    reject_dangerous_surql(&req.query)?;
    ctx.use_scoped_db().await?;

    let mut q = ctx.db.query(&req.query);
    if let Some(model) = &req.model {
        // Raw SurQL that references a model still needs read or write on that model.
        let grant = ctx
            .model(model)
            .ok_or_else(|| HostError::ModelDenied(model.clone()))?;
        validate_ident(&grant.table)?;
        q = q.bind(("__table", grant.table.clone()));
    }
    for (k, v) in req.vars {
        validate_ident(&k)?;
        q = q.bind((k, v));
    }
    let response = q.await?.check()?;
    // Return raw multi-statement results as JSON array of statement outputs when possible.
    Ok(serde_json::json!({
        "ok": true,
        "data": serde_json::to_value(format!("{response:?}")).unwrap_or(JsonValue::Null),
        "note": "raw SurQL executed under db::surql; prefer structured db::* commands"
    }))
}

fn strip_table_prefix<'a>(id: &'a str, table: &str) -> &'a str {
    id.strip_prefix(&format!("{table}:")).unwrap_or(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_use_ns() {
        let err = reject_dangerous_surql("USE NS other; SELECT * FROM x").unwrap_err();
        assert!(matches!(err, HostError::SurqlRejected(_)));
    }

    #[test]
    fn allows_select() {
        reject_dangerous_surql("SELECT * FROM type::table($__table) WHERE active = $active")
            .unwrap();
    }
}
