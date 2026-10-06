//! Trees and relations kept as SurrealDB graph edges.
//!
//! Two kinds of field are graphs:
//!
//! * a **hierarchy** link (`"type": "link", "target": <own model>, "hierarchy": true`) is a
//!   parent pointer. The column stays the source of truth and stays cheap to read, and the kernel
//!   keeps an edge `parent -> child` in step with it inside the same transaction as every write.
//!   That lets [`db_tree`] walk ancestors or descendants to any depth in one query, and lets the
//!   kernel refuse a parent that does not exist, a loop, and the deletion of a record that still
//!   has children.
//! * a **many2many** field has no column at all, only edges from this model's records to records
//!   of its `target`; [`db_relate`], [`db_unrelate`] and [`db_related`] change and read them.
//!
//! Edge tables are `ENFORCED` relations, so the database itself refuses an edge to a record that
//! does not exist, and deleting a record removes its edges. Plugins never name an edge table:
//! they name a field of a model they were granted, so the usual grants and audit apply.

use serde::Deserialize;
use serde_json::{Map, Value as JsonValue};

use super::context::{ModelGrant, PluginHostContext};
use super::guard;
use crate::data_model::Operation;
use super::db::{Access, Extra, RECORD_ID, decode, parse_req, record_binds, require_model, run_audited, strip_table_prefix, validate_ident};
use super::error::HostError;
use crate::data_model::ModelSchema;

/// The deepest tree walk, whatever the plugin asks for.
pub const MAX_DEPTH: u32 = 64;
/// Records one `db::relate` / `db::unrelate` may name.
pub const MAX_RELATED_PER_CALL: usize = 100;
/// Records one `db::tree` / `db::related` returns.
const MAX_ROWS: u32 = 1000;

/// Bind names for the parent of a hierarchy field; the `d<number>` form is renamed per write
/// inside a transaction, like the update values.
fn parent_bind(index: usize) -> String {
    format!("d{}", 900 + index)
}

/// Bind names for the links whose targets are checked; renamed per write like the hierarchy ones.
fn link_bind(index: usize, part: usize) -> String {
    format!("d{}", 800 + index * 2 + part)
}

/// Refuse a write that points a plain link at a record that does not exist. `values` is the
/// written columns (by field id); the check runs before the write, in the same transaction.
pub(super) fn link_checks(
    schema: &ModelSchema,
    values: &Map<String, JsonValue>,
    binds: &mut Vec<(String, JsonValue)>,
    extra: &mut Extra,
) {
    for (index, link) in schema.links.iter().enumerate() {
        let Some(target) = values.get(&link.column).and_then(JsonValue::as_str) else { continue };
        let (table_bind, key_bind) = (link_bind(index, 0), link_bind(index, 1));
        binds.push((table_bind.clone(), JsonValue::String(link.table.clone())));
        binds.push((key_bind.clone(), JsonValue::String(strip_table_prefix(target, &link.table).to_string())));
        extra.before.push(format!(
            "IF !record::exists(type::record(${table_bind}, ${key_bind})) {{ THROW 'aether: the record `{}` points to does not exist'; }};",
            link.field
        ));
    }
}

/// What a create must also do for its hierarchy links: check the parent exists and add the edge.
pub(super) fn create_extra(schema: &ModelSchema, data: &Map<String, JsonValue>, binds: &mut Vec<(String, JsonValue)>, extra: &mut Extra) {
    for (index, hierarchy) in schema.hierarchies.iter().enumerate() {
        let Some(parent) = data.get(&hierarchy.column).and_then(JsonValue::as_str) else { continue };
        let bind = parent_bind(index);
        binds.push((bind.clone(), JsonValue::String(strip_table_prefix(parent, &schema.table).to_string())));
        extra.after.push(format!(
            "IF !record::exists(type::record($__table, ${bind})) {{ THROW 'aether: the parent of `{}` does not exist'; }};",
            hierarchy.field
        ));
        extra.after.push(format!(
            "RELATE (type::record($__table, ${bind}))->{}->($rows[0].id);",
            hierarchy.edge
        ));
    }
}

/// What an update must also do when it moves a record: refuse a missing parent or a loop (the
/// new parent being the record itself or one of its descendants), then replace the edge.
pub(super) fn update_extra(
    schema: &ModelSchema,
    set: &Map<String, JsonValue>,
    clear: &[String],
    binds: &mut Vec<(String, JsonValue)>,
    extra: &mut Extra,
) {
    for (index, hierarchy) in schema.hierarchies.iter().enumerate() {
        let cleared = clear.contains(&hierarchy.column);
        let parent = set.get(&hierarchy.column).and_then(JsonValue::as_str);
        if !cleared && parent.is_none() {
            continue;
        }
        if let Some(parent) = parent {
            let bind = parent_bind(index);
            binds.push((bind.clone(), JsonValue::String(strip_table_prefix(parent, &schema.table).to_string())));
            let new_parent = format!("type::record($__table, ${bind})");
            let me = "type::record($__table, $__key)";
            extra.before.push(format!(
                "IF !record::exists({new_parent}) {{ THROW 'aether: the parent of `{}` does not exist'; }};",
                hierarchy.field
            ));
            extra.before.push(format!(
                "IF {new_parent} = {me} OR {new_parent} IN {me}.{{..{MAX_DEPTH}+collect}}->{}->{} \
                 {{ THROW 'aether: `{}` would make a loop: the new parent is the record itself or below it'; }};",
                hierarchy.edge, schema.table, hierarchy.field
            ));
        }
        extra.after.push(format!("DELETE {} WHERE out = type::record($__table, $__key);", hierarchy.edge));
        if parent.is_some() {
            let bind = parent_bind(index);
            extra.after.push(format!(
                "RELATE (type::record($__table, ${bind}))->{}->(type::record($__table, $__key));",
                hierarchy.edge
            ));
        }
    }
}

/// What a delete must check: a record that still has children is not deleted, so no child is left
/// pointing at nothing. Move or delete the children first.
pub(super) fn delete_extra(schema: &ModelSchema, extra: &mut Extra) {
    for hierarchy in &schema.hierarchies {
        extra.before.push(format!(
            "IF array::len((SELECT VALUE id FROM {} WHERE in = type::record($__table, $__key) LIMIT 1)) > 0 \
             {{ THROW 'aether: the record still has children under `{}`: move or delete them first'; }};",
            hierarchy.edge, hierarchy.field
        ));
    }
}

#[derive(Debug, Deserialize)]
pub struct RelateRequest {
    pub model: String,
    pub field: String,
    pub id: String,
    pub to: Vec<String>,
}

/// Which records the edges are between: the target record's table must be the field's target.
fn target_keys(table: &str, ids: &[String]) -> Result<Vec<String>, HostError> {
    if ids.is_empty() || ids.len() > MAX_RELATED_PER_CALL {
        return Err(HostError::InvalidPayload(format!("`to` holds 1 to {MAX_RELATED_PER_CALL} record ids")));
    }
    ids.iter()
        .map(|id| {
            let key = id
                .strip_prefix(&format!("{table}:"))
                .filter(|key| !key.is_empty())
                .ok_or_else(|| HostError::InvalidPayload(format!("`{id}` is not a record of the field's target model")))?;
            Ok(key.to_string())
        })
        .collect()
}

async fn change_edges(ctx: &PluginHostContext, payload: &JsonValue, add: bool) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::mutate")?;
    let req: RelateRequest = parse_req(payload)?;
    let grant = require_model(ctx, &req.model, true)?;
    let schema = grant
        .schema
        .as_ref()
        .ok_or_else(|| HostError::InvalidPayload(format!("model `{}` has no definition, so it has no relations", req.model)))?;
    let relation = schema.relation(&req.field)?;
    validate_ident(&relation.edge)?;
    validate_ident(&relation.target_table)?;
    let keys = target_keys(&relation.target_table, &req.to)?;
    let scope = guard::scope(ctx, grant, Operation::Write, "r").await?.ok_or_else(|| guard::denied(&req.model, "change"))?;

    let mut binds = record_binds(grant, &req.id);
    binds.extend(scope.binds);
    binds.push(("__target".to_string(), JsonValue::String(relation.target_table.clone())));
    binds.push(("__to".to_string(), JsonValue::from(keys)));
    let mut extra = Extra::default();
    extra.before.push(
        "IF !record::exists(type::record($__table, $__key)) { THROW 'aether: there is no such record'; };".into(),
    );
    if !scope.sql.is_empty() {
        extra.before.push(format!(
            "IF array::len((SELECT VALUE id FROM type::record($__table, $__key) WHERE {})) = 0 \
             {{ THROW 'aether-denied: you may not change this `{}` record'; }};",
            scope.sql, req.model
        ));
    }
    let me = "type::record($__table, $__key)";
    extra.before.push(if add {
        format!(
            "FOR $k IN $__to {{ LET $t = type::record($__target, $k); \
             IF !record::exists($t) {{ THROW 'aether: a record to relate to does not exist'; }}; \
             IF array::len((SELECT VALUE id FROM {edge} WHERE in = {me} AND out = $t LIMIT 1)) = 0 {{ RELATE ({me})->{edge}->$t; }}; }};",
            edge = relation.edge
        )
    } else {
        format!(
            "FOR $k IN $__to {{ DELETE {edge} WHERE in = {me} AND out = type::record($__target, $k); }};",
            edge = relation.edge
        )
    });
    let rows = run_audited(
        ctx,
        Access { operation: "update", model: req.model.clone(), table: &grant.table },
        "LET $rows = (SELECT * FROM type::record($__table, $__key));",
        RECORD_ID,
        binds,
        extra,
    )
    .await?;
    Ok(serde_json::json!({ "ok": true, "data": rows.into_iter().next().map(|row| decode(grant, row)) }))
}

/// Link the record `id` of `model` to each of `to` through the many2many field `field`. Linking
/// twice is harmless. Needs write access to `model`.
pub async fn db_relate(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    change_edges(ctx, payload, true).await
}

/// Remove those links (links that do not exist are ignored).
pub async fn db_unrelate(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    change_edges(ctx, payload, false).await
}

#[derive(Debug, Deserialize)]
pub struct RelatedRequest {
    pub model: String,
    pub field: String,
    pub id: String,
    /// Read the other way: `id` is a record of the field's target, and the answer is the records
    /// of `model` linked to it.
    #[serde(default)]
    pub reverse: bool,
    #[serde(default)]
    pub limit: Option<u32>,
}

/// The records linked to `id` through a many2many field. Forwards, they are records of the
/// field's target model, which the plugin must be able to read; in reverse, records of `model`.
pub async fn db_related(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::query")?;
    let req: RelatedRequest = parse_req(payload)?;
    let owner = require_model(ctx, &req.model, false)?;
    let schema = owner
        .schema
        .as_ref()
        .ok_or_else(|| HostError::InvalidPayload(format!("model `{}` has no definition, so it has no relations", req.model)))?;
    let relation = schema.relation(&req.field)?;
    validate_ident(&relation.edge)?;

    // The records that come back, and the table the given `id` is in.
    let (result, given_table) = if req.reverse {
        (owner, relation.target_table.as_str())
    } else {
        // A plugin cannot always read the other side (a model of another plugin), but the links are
        // its own: it gets the ids, which is what it needs to ask that plugin about them.
        let Some(target) = ctx.models.values().find(|grant| grant.table == relation.target_table) else {
            return linked_ids(ctx, owner, &relation.edge, &req).await;
        };
        let target = require_model(ctx, &target.name, false)?;
        (target, owner.table.as_str())
    };
    let key = req
        .id
        .strip_prefix(&format!("{given_table}:"))
        .filter(|key| !key.is_empty())
        .ok_or_else(|| HostError::InvalidPayload(format!("`{}` is not a record of `{given_table}`", req.id)))?;
    let traversal = if req.reverse { format!("<-{}<-?", relation.edge) } else { format!("->{}->?", relation.edge) };
    let limit = req.limit.map_or(MAX_ROWS, |limit| limit.min(MAX_ROWS));
    // The records that come back are the result model's, so its rules say which may be seen.
    let Some(scope) = guard::scope(ctx, result, Operation::Read, "r").await? else {
        return Ok(serde_json::json!({ "ok": true, "data": [] }));
    };
    let (hidden, _) = guard::field_limits(ctx, result).await?;
    let filter = if scope.sql.is_empty() { String::new() } else { format!(" WHERE {}", scope.sql) };
    let body = format!(
        "LET $rows = (SELECT * FROM (type::record($__given, $__key){traversal}){filter} LIMIT {limit});"
    );
    let mut binds = scope.binds;
    binds.extend(vec![
        ("__given".to_string(), JsonValue::String(given_table.to_string())),
        ("__key".to_string(), JsonValue::String(key.to_string())),
    ]);
    binds.push(("__table".to_string(), JsonValue::String(result.table.clone())));
    let rows = run_audited(
        ctx,
        Access { operation: "read", model: result.name.clone(), table: &result.table },
        &body,
        "$rows.id",
        binds,
        Extra::default(),
    )
    .await?;
    let rows: Vec<JsonValue> = rows.into_iter().map(|row| guard::strip(&hidden, decode(result, row))).collect();
    Ok(serde_json::json!({ "ok": true, "data": rows }))
}

/// The ids a record is linked to, as `{ "id": "table:key" }` rows.
async fn linked_ids(ctx: &PluginHostContext, owner: &ModelGrant, edge: &str, req: &RelatedRequest) -> Result<JsonValue, HostError> {
    let key = req
        .id
        .strip_prefix(&format!("{}:", owner.table))
        .filter(|key| !key.is_empty())
        .ok_or_else(|| HostError::InvalidPayload(format!("`{}` is not a record of `{}`", req.id, owner.table)))?;
    let limit = req.limit.map_or(MAX_ROWS, |limit| limit.min(MAX_ROWS));
    let body = format!("LET $rows = (SELECT id FROM (type::record($__given, $__key)->{edge}->?) LIMIT {limit});");
    let binds = vec![
        ("__given".to_string(), JsonValue::String(owner.table.clone())),
        ("__key".to_string(), JsonValue::String(key.to_string())),
        ("__table".to_string(), JsonValue::String(owner.table.clone())),
    ];
    let rows = run_audited(ctx, Access { operation: "read", model: owner.name.clone(), table: &owner.table }, &body, "$rows.id", binds, Extra::default()).await?;
    let ids: Vec<JsonValue> = rows
        .into_iter()
        .filter_map(|row| row.get("id").cloned())
        .map(|id| serde_json::json!({ "id": id }))
        .collect();
    Ok(serde_json::json!({ "ok": true, "data": ids }))
}

#[derive(Debug, Deserialize)]
pub struct TreeRequest {
    pub model: String,
    pub field: String,
    pub id: String,
    /// `down` (descendants, the default) or `up` (ancestors, nearest first is not promised).
    #[serde(default)]
    pub direction: Option<String>,
    /// How many levels to walk; every level by default, at most [`MAX_DEPTH`].
    #[serde(default)]
    pub depth: Option<u32>,
    /// Include the record itself.
    #[serde(default)]
    pub include_self: bool,
}

/// Descendants (`down`) or ancestors (`up`) of a record through a hierarchy link, as records of
/// `model`, to the depth asked for.
pub async fn db_tree(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::query")?;
    let req: TreeRequest = parse_req(payload)?;
    let grant = require_model(ctx, &req.model, false)?;
    let schema = grant
        .schema
        .as_ref()
        .ok_or_else(|| HostError::InvalidPayload(format!("model `{}` has no definition, so it has no hierarchy", req.model)))?;
    let hierarchy = schema.hierarchy(&req.field)?;
    validate_ident(&hierarchy.edge)?;
    validate_ident(&grant.table)?;
    let depth = req.depth.unwrap_or(MAX_DEPTH).clamp(1, MAX_DEPTH);
    let arrow = match req.direction.as_deref() {
        None | Some("down") => format!("->{}->{}", hierarchy.edge, grant.table),
        Some("up") => format!("<-{}<-{}", hierarchy.edge, grant.table),
        Some(other) => return Err(HostError::InvalidPayload(format!("`direction` is `down` or `up`, not `{other}`"))),
    };
    let me = "type::record($__table, $__key)";
    let found = format!("{me}.{{..{depth}+collect}}{arrow}");
    let ids = if req.include_self { format!("array::prepend({found}, {me})") } else { found };
    let Some(scope) = guard::scope(ctx, grant, Operation::Read, "r").await? else {
        return Ok(serde_json::json!({ "ok": true, "data": [] }));
    };
    let (hidden, _) = guard::field_limits(ctx, grant).await?;
    let filter = if scope.sql.is_empty() { String::new() } else { format!(" WHERE {}", scope.sql) };
    let body = format!("LET $rows = (SELECT * FROM {ids}{filter} LIMIT {MAX_ROWS});");
    let mut binds = record_binds(grant, &req.id);
    binds.extend(scope.binds);
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
