use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as JsonValue};

use crate::data_model::query::{Unchecked, compile_filter};
use crate::data_model::{Aggregate, Filter};

use super::context::{ModelGrant, PluginHostContext};
use super::error::HostError;
use super::guard;
use crate::data_model::Operation;

/// Soft TTL for ACL-safe active row fills from `db::get`.
const ROW_CACHE_TTL: Duration = Duration::from_secs(600);

/// Soft TTL for cached `db::find` / `db::count` / `db::aggregate` results. Writes through the
/// host bump the table generation; the TTL only covers writes made outside this process.
const QUERY_CACHE_TTL: Duration = Duration::from_secs(120);

fn plugin_cache_ns(ctx: &PluginHostContext) -> String {
    format!("plugin:{}:{}", ctx.database, ctx.plugin_name)
}

/// Only cache full rows when the caller has no row-scope SQL and no hidden fields.
fn row_cache_safe(scope_sql: &str, hidden: &std::collections::HashSet<String>) -> bool {
    scope_sql.is_empty() && hidden.is_empty()
}

/// One key for a row, whether the caller passed `key` or `table:key`.
/// A form route uses the bare key; a saved record's `id` is `table:key`. Those
/// must hit the same cache entry or a write leaves the form on a stale row.
fn row_cache_id(table: &str, id: &str) -> String {
    format!("{table}:{}", strip_table_prefix(id, table))
}

/// Spellings to drop: the canonical id, the id the caller passed, and the bare key.
fn row_cache_keys(table: &str, id: &str) -> Vec<String> {
    let canonical = row_cache_id(table, id);
    let mut keys = vec![canonical.clone()];
    if id != canonical {
        keys.push(id.to_string());
    }
    let bare = strip_table_prefix(id, table);
    if bare != canonical && bare != id {
        keys.push(bare.to_string());
    }
    keys
}

/// Current write generation of `table` in this organization. Cached rows and query results carry
/// the generation they were read at; any write bumps it, so a stale entry can never be served,
/// whichever plugin (or code path) did the write.
fn table_gen(ctx: &PluginHostContext, table: &str) -> Option<u64> {
    let services = ctx.services().ok()?;
    Some(services.cache.table_gen(&ctx.database, table))
}

/// A cached read, with what the audit trail needs to record it again on a hit.
struct Cached {
    data: JsonValue,
    /// `table:key` of every record the original read returned.
    ids: Vec<String>,
    /// Rows the original statement produced (`record_count` of the audit row).
    n: usize,
}

fn gen_wrap(stamp: u64, data: &JsonValue, ids: &[String], n: usize) -> Option<Vec<u8>> {
    serde_json::to_vec(&serde_json::json!({ "g": stamp, "d": data, "ids": ids, "n": n })).ok()
}

fn gen_unwrap(bytes: &[u8], stamp: u64) -> Option<Cached> {
    let mut value: JsonValue = serde_json::from_slice(bytes).ok()?;
    if value.get("g").and_then(JsonValue::as_u64)? != stamp {
        return None;
    }
    let ids = value
        .get("ids")?
        .as_array()?
        .iter()
        .filter_map(|id| id.as_str().map(str::to_string))
        .collect();
    let n = value.get("n").and_then(JsonValue::as_u64)? as usize;
    Some(Cached {
        data: value.get_mut("d")?.take(),
        ids,
        n,
    })
}

/// Record a read that was answered from Mantle. Same row as an uncached read writes, minus the
/// `SELECT`; if it cannot be written the read fails, exactly as an uncached one would.
async fn audit_cached(
    ctx: &PluginHostContext,
    model: &str,
    table: &str,
    cached: &Cached,
) -> Result<(), HostError> {
    let query = ctx
        .db
        .query(
            r#"
            CREATE data_access SET
                request_id = $__audit_request,
                actor_type = $__audit_actor_type,
                actor_id = $__audit_actor_id,
                plugin = $__audit_plugin,
                function_name = $__audit_function,
                model = $__audit_model,
                table_name = $__audit_table,
                operation = 'read',
                record_ids = array::map($__ids, |$i| type::record($i)),
                record_count = $__count,
                ip = $__audit_ip;
            "#,
        )
        .bind(("__audit_request", ctx.audit.request_id.clone()))
        .bind(("__audit_actor_type", ctx.audit.actor.kind().to_string()))
        .bind(("__audit_actor_id", ctx.audit.actor.id().map(str::to_string)))
        .bind(("__audit_plugin", ctx.plugin_name.clone()))
        .bind(("__audit_function", ctx.function.clone()))
        .bind(("__audit_model", model.to_string()))
        .bind(("__audit_table", table.to_string()))
        .bind(("__ids", cached.ids.clone()))
        .bind(("__count", cached.n as i64))
        .bind(("__audit_ip", ctx.audit.ip.clone()));
    checked(query.await.map_err(map_db_error)?)?;
    Ok(())
}

/// Ids of the records in `rows`, as the audit row stores them.
pub(super) fn record_ids_of(rows: &[JsonValue]) -> Vec<String> {
    rows.iter()
        .filter_map(|row| record_id_of(row).map(str::to_string))
        .collect()
}

/// Wait for any fill of `key` already under way (and re-check the cache afterwards).
async fn flight(ctx: &PluginHostContext, key: &str) -> Option<crate::cache::Flight> {
    let services = ctx.services().ok()?;
    Some(services.cache.flight(key).await)
}

fn mantle_get_row(
    ctx: &PluginHostContext,
    model: &str,
    table: &str,
    id: &str,
) -> Option<Cached> {
    let services = ctx.services().ok()?;
    let stamp = table_gen(ctx, table)?;
    let value = services
        .cache
        .get_row(&plugin_cache_ns(ctx), model, &row_cache_id(table, id))
        .ok()??;
    gen_unwrap(value.as_slice(), stamp)
}

fn mantle_put_row(
    ctx: &PluginHostContext,
    model: &str,
    table: &str,
    id: &str,
    stamp: u64,
    data: &JsonValue,
    ids: &[String],
    n: usize,
) {
    let Ok(services) = ctx.services() else {
        return;
    };
    let Some(bytes) = gen_wrap(stamp, data, ids, n) else {
        return;
    };
    let _ = services.cache.set_row(
        &plugin_cache_ns(ctx),
        model,
        &row_cache_id(table, id),
        bytes,
        Some(ROW_CACHE_TTL),
    );
}

/// Other nodes keep their own L1: tell them this table changed.
pub(super) fn announce(ctx: &PluginHostContext, table: &str) {
    let Ok(services) = ctx.services() else {
        return;
    };
    if !services.cache.coherence_signals() {
        return;
    }
    if let Some(core) = &services.core {
        crate::plugin_manager::mantle_signals::announce_table_write(
            core.clone(),
            &ctx.database,
            table,
        );
    }
}

/// A write to `table`: bump its generation (every cached row, list and aggregate of the table,
/// in every plugin, is now stale) and drop the written row's own entries.
fn mantle_bust_row(ctx: &PluginHostContext, model: &str, table: &str, id: Option<&str>) {
    let Ok(services) = ctx.services() else {
        return;
    };
    services.cache.bump_table_gen(&ctx.database, table);
    announce(ctx, table);
    // A written child row also rewrites the totals of the records it belongs to.
    if let Some(schema) = ctx.models.get(model).and_then(|grant| grant.schema.as_ref()) {
        for link in &schema.parents {
            services.cache.bump_table_gen(&ctx.database, &link.schema.table);
            announce(ctx, &link.schema.table);
        }
    }
    let Some(id) = id else {
        return;
    };
    let ns = plugin_cache_ns(ctx);
    for key in row_cache_keys(table, id) {
        let _ = services.cache.invalidate_row(&ns, model, &key);
    }
}

/// Stable key for a query: the exact SQL, its binds and the fields hidden from the caller.
/// Two callers get the same entry only when they would run the same statement and see the
/// same fields, so the row-scope rules stay respected.
pub(super) fn query_hash(
    table: &str,
    body: &str,
    binds: &[(String, JsonValue)],
    hidden: &std::collections::HashSet<String>,
) -> String {
    use sha2::{Digest, Sha256};
    let mut hidden: Vec<&String> = hidden.iter().collect();
    hidden.sort();
    let mut hasher = Sha256::new();
    hasher.update(table.as_bytes());
    hasher.update([0]);
    hasher.update(body.as_bytes());
    hasher.update([0]);
    hasher.update(serde_json::to_vec(binds).unwrap_or_default());
    hasher.update([0]);
    hasher.update(serde_json::to_vec(&hidden).unwrap_or_default());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn mantle_get_list(
    ctx: &PluginHostContext,
    model: &str,
    table: &str,
    hash: &str,
) -> Option<Cached> {
    let services = ctx.services().ok()?;
    let stamp = table_gen(ctx, table)?;
    let value = services
        .cache
        .get_list(&plugin_cache_ns(ctx), model, hash)
        .ok()??;
    gen_unwrap(value.as_slice(), stamp)
}

fn mantle_put_list(
    ctx: &PluginHostContext,
    model: &str,
    hash: &str,
    stamp: u64,
    data: &JsonValue,
    ids: &[String],
    n: usize,
) {
    let Ok(services) = ctx.services() else {
        return;
    };
    let Some(bytes) = gen_wrap(stamp, data, ids, n) else {
        return;
    };
    let _ = services.cache.set_list(
        &plugin_cache_ns(ctx),
        model,
        hash,
        bytes,
        Some(QUERY_CACHE_TTL),
    );
}

fn mantle_get_agg(
    ctx: &PluginHostContext,
    model: &str,
    table: &str,
    name: &str,
    hash: &str,
) -> Option<Cached> {
    let services = ctx.services().ok()?;
    let stamp = table_gen(ctx, table)?;
    let value = services
        .cache
        .get_agg(&plugin_cache_ns(ctx), model, name, hash)
        .ok()??;
    gen_unwrap(value.as_slice(), stamp)
}

fn mantle_put_agg(
    ctx: &PluginHostContext,
    model: &str,
    name: &str,
    hash: &str,
    stamp: u64,
    data: &JsonValue,
    n: usize,
) {
    let Ok(services) = ctx.services() else {
        return;
    };
    let Some(bytes) = gen_wrap(stamp, data, &[], n) else {
        return;
    };
    let _ = services.cache.set_agg(
        &plugin_cache_ns(ctx),
        model,
        name,
        hash,
        bytes,
        Some(QUERY_CACHE_TTL),
    );
}

/// Newest write generation among `tables`. A write to any of them raises it, because a bump
/// always lands above every value handed out before.
fn tables_stamp(ctx: &PluginHostContext, tables: &[&str]) -> Option<u64> {
    let services = ctx.services().ok()?;
    tables
        .iter()
        .map(|table| services.cache.table_gen(&ctx.database, table))
        .max()
}

/// Run a read that touches several tables behind Mantle: serve a fresh entry (still auditing
/// the read), let one caller fill a missing one, and stamp the result with the generation seen
/// *before* the query. `fill` returns the data to cache, the ids read and the row count.
pub(super) async fn cached_read<F, Fut>(
    ctx: &PluginHostContext,
    model: &str,
    audit_table: &str,
    tables: &[&str],
    name: &'static str,
    hash: &str,
    fill: F,
) -> Result<JsonValue, HostError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(JsonValue, Vec<String>, usize), HostError>>,
{
    let Some(services) = ctx.services().ok() else {
        return Ok(fill().await?.0);
    };
    let ns = plugin_cache_ns(ctx);
    let lookup = |stamp: u64| -> Option<Cached> {
        let value = services.cache.get_agg(&ns, model, name, hash).ok()??;
        gen_unwrap(value.as_slice(), stamp)
    };
    let Some(stamp) = tables_stamp(ctx, tables) else {
        return Ok(fill().await?.0);
    };
    if let Some(cached) = lookup(stamp) {
        audit_cached(ctx, model, audit_table, &cached).await?;
        return Ok(cached.data);
    }
    let _flight = services
        .cache
        .flight(&format!("{ns}|agg|{model}|{name}|{hash}"))
        .await;
    // Re-read after waiting: whoever held the flight has stored it, or a write moved on.
    let stamp = tables_stamp(ctx, tables).unwrap_or(stamp);
    if let Some(cached) = lookup(stamp) {
        audit_cached(ctx, model, audit_table, &cached).await?;
        return Ok(cached.data);
    }
    let (data, ids, n) = fill().await?;
    if let Some(bytes) = gen_wrap(stamp, &data, &ids, n) {
        let _ = services
            .cache
            .set_agg(&ns, model, name, hash, bytes, Some(QUERY_CACHE_TTL));
    }
    Ok(data)
}

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
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(HostError::InvalidPayload(format!(
            "invalid identifier `{name}`"
        )));
    }
    Ok(())
}

fn validate_field_name(name: &str) -> Result<(), HostError> {
    validate_ident(name)
}

pub(super) fn parse_req<T: serde::de::DeserializeOwned>(
    payload: &JsonValue,
) -> Result<T, HostError> {
    serde_json::from_value(payload.clone()).map_err(|e| HostError::InvalidPayload(e.to_string()))
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
    let Some(schema) = &grant.schema else {
        return Ok(Extra::default());
    };
    let Some(chatter) = &schema.chatter else {
        return Ok(Extra::default());
    };
    let line = "CREATE chatter_messages SET ulid = rand::ulid(), model_id = $__table, \
                author = $__cm_author, plugin = $__audit_plugin";
    let mut extra = Extra::default();
    match operation {
        "create" => {
            let key = "<string> record::id($rows[0].id)";
            extra.after.push(format!(
                "{line}, record_key = {key}, kind = 'system', body = 'created';"
            ));
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
            extra.before.push(
                "LET $__before = (SELECT * FROM ONLY type::record($__table, $__key));".into(),
            );
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
            extra.after.push(
                "DELETE chatter_followers WHERE model_id = $__table AND record_key = $__key;"
                    .into(),
            );
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
        .bind((
            "__cm_author",
            ctx.audit.actor.id().unwrap_or("system").to_string(),
        ));
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
        Some((_, message)) => {
            HostError::InvalidPayload(message.trim_end_matches(['\'', '"']).to_string())
        }
        None => HostError::Db(error),
    }
}

/// A response with every statement succeeded. When one fails inside a transaction the others
/// come back as "not executed", so the statement that actually failed is the one to report.
fn checked(
    mut response: surrealdb::IndexedResults,
) -> Result<surrealdb::IndexedResults, HostError> {
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
        (
            "__table".to_string(),
            JsonValue::String(grant.table.clone()),
        ),
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

/// Tell connected browsers that a model row changed (transient SSE; no plugin listeners).
fn publish_data_changed(ctx: &PluginHostContext, model: &str, op: &str, id: Option<&str>) {
    if ctx.database.is_empty() || model.is_empty() {
        return;
    }
    ctx.notifications.publish(
        &ctx.database,
        crate::notifications::HubMessage::Event(std::sync::Arc::new(
            crate::notifications::UiEvent {
                plugin: "aether".into(),
                event: "data.changed".into(),
                payload: serde_json::json!({
                    "model": model,
                    "op": op,
                    "id": id,
                }),
            },
        )),
    );
}

fn mutate_op(operation: &str) -> Option<&'static str> {
    match operation {
        "create" => Some("create"),
        "update" => Some("update"),
        "delete" => Some("delete"),
        _ => None,
    }
}

fn record_id_of(data: &JsonValue) -> Option<&str> {
    data.get("id").and_then(JsonValue::as_str)
}

async fn run_plan(ctx: &PluginHostContext, plan: Plan<'_>) -> Result<JsonValue, HostError> {
    let model = plan.access.model.clone();
    let op = mutate_op(plan.access.operation);
    let grant = plan.grant;
    let rows = run_audited(
        ctx,
        plan.access,
        &plan.body,
        &plan.ids,
        plan.binds,
        plan.extra,
    )
    .await?;
    let (hidden, _) = guard::field_limits(ctx, grant).await?;
    let data = rows
        .into_iter()
        .next()
        .map(|row| guard::strip(&hidden, decode(grant, row)));
    if let Some(op) = op {
        let id = data.as_ref().and_then(record_id_of);
        publish_data_changed(ctx, &model, op, id);
        mantle_bust_row(ctx, &model, &grant.table, id);
    }
    Ok(serde_json::json!({ "ok": true, "data": data }))
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
    // ACL-safe Mantle hit: unrestricted row scope, no hidden fields, no expand.
    let cacheable = req.expand.is_empty() && row_cache_safe(&scope.sql, &hidden);
    let mut _flight = None;
    if cacheable {
        if let Some(cached) = mantle_get_row(ctx, &req.model, &grant.table, &req.id) {
            audit_cached(ctx, &req.model, &grant.table, &cached).await?;
            return Ok(serde_json::json!({ "ok": true, "data": cached.data }));
        }
        // One caller fills the entry; the rest wait and find it.
        let key = format!(
            "{}|row|{}|{}",
            plugin_cache_ns(ctx),
            req.model,
            row_cache_id(&grant.table, &req.id)
        );
        _flight = flight(ctx, &key).await;
        if _flight.is_some()
            && let Some(cached) = mantle_get_row(ctx, &req.model, &grant.table, &req.id)
        {
            audit_cached(ctx, &req.model, &grant.table, &cached).await?;
            return Ok(serde_json::json!({ "ok": true, "data": cached.data }));
        }
    }
    // Read the generation before the query: a write that lands meanwhile makes the entry stale.
    let stamp_before = table_gen(ctx, &grant.table);
    let mut binds = record_binds(grant, &req.id);
    binds.extend(scope.binds);
    let body = if scope.sql.is_empty() {
        "LET $rows = SELECT * FROM type::record($__table, $__key);".to_string()
    } else {
        format!(
            "LET $rows = SELECT * FROM type::record($__table, $__key) WHERE {};",
            scope.sql
        )
    };
    let rows = run_audited(
        ctx,
        Access {
            operation: "read",
            model: req.model.clone(),
            table: &grant.table,
        },
        &body,
        RECORD_ID,
        binds,
        Extra::default(),
    )
    .await?;
    let mut records: Vec<JsonValue> = rows
        .into_iter()
        .take(1)
        .map(|row| guard::strip(&hidden, decode(grant, row)))
        .collect();
    if let (Some(schema), false) = (&grant.schema, req.expand.is_empty()) {
        super::nested::expand(ctx, schema, &mut records, &req.expand).await?;
    }
    let data = records.into_iter().next();
    if let Some(ref row) = data {
        if req.expand.is_empty() && row_cache_safe(&scope.sql, &hidden) && !row.is_null() {
            if let Some(stamp) = stamp_before {
                let ids = record_ids_of(std::slice::from_ref(row));
                mantle_put_row(ctx, &req.model, &grant.table, &req.id, stamp, row, &ids, 1);
            }
        }
    }
    Ok(serde_json::json!({ "ok": true, "data": data }))
}

/// The `WHERE` condition for a filter over `grant`'s model, with the values it binds (the table
/// is always bound as `$__table`), narrowed by what the model's rules let the caller read. The
/// condition is empty when everything matches. `None`: the rules let the caller read nothing.
/// Also answers which fields the caller may not see.
pub(super) async fn guarded_condition(
    ctx: &PluginHostContext,
    grant: &ModelGrant,
    model: &str,
    filter: &Map<String, JsonValue>,
) -> Result<
    Option<(
        String,
        Vec<(String, JsonValue)>,
        std::collections::HashSet<String>,
    )>,
    HostError,
> {
    let parsed = Filter::parse(filter)?;
    let (hidden, _) = guard::field_limits(ctx, grant).await?;
    guard::check_unreadable(&hidden, parsed.fields(), model)?;
    let compiled = match &grant.schema {
        Some(schema) => compile_filter(&parsed, schema.as_ref(), "f")?,
        None => compile_filter(&parsed, &Unchecked, "f")?,
    };
    let Some(scope) = guard::scope(ctx, grant, Operation::Read, "r").await? else {
        return Ok(None);
    };
    let mut binds = vec![(
        "__table".to_string(),
        JsonValue::String(grant.table.clone()),
    )];
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
pub struct SearchRequest {
    pub model: String,
    /// Free-text needle; expanded to `or` of `{ field: { like: q } }` over `fields`.
    #[serde(default)]
    pub q: Option<String>,
    /// Fields to search when `q` is set. Empty + non-empty `q` is an error.
    #[serde(default)]
    pub fields: Vec<String>,
    #[serde(default)]
    pub filter: Map<String, JsonValue>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub offset: Option<u32>,
    #[serde(default)]
    pub order: Option<String>,
    #[serde(default)]
    pub expand: Vec<String>,
}

/// Merge a free-text `q` into a filter as an OR of case-insensitive `like` clauses.
///
/// Empty / whitespace `q` leaves `filter` unchanged. Non-empty `q` with no `fields` is an error.
pub fn expand_search_filter(
    filter: Map<String, JsonValue>,
    q: Option<&str>,
    fields: &[String],
) -> Result<Map<String, JsonValue>, HostError> {
    let needle = q.map(str::trim).filter(|v| !v.is_empty());
    let Some(needle) = needle else {
        return Ok(filter);
    };
    if fields.is_empty() {
        return Err(HostError::InvalidPayload(
            "db::search: `q` is set but `fields` is empty".into(),
        ));
    }
    for field in fields {
        let trimmed = field.trim();
        if trimmed.is_empty() {
            return Err(HostError::InvalidPayload(
                "db::search: empty field name in `fields`".into(),
            ));
        }
        validate_field_name(trimmed)?;
    }
    let likes: Vec<JsonValue> = fields
        .iter()
        .map(|field| {
            let name = field.trim();
            serde_json::json!({ name: { "like": needle } })
        })
        .collect();
    if filter.is_empty() {
        return Ok(Map::from_iter([(
            "or".to_string(),
            JsonValue::Array(likes),
        )]));
    }
    let text_clause = JsonValue::Object(Map::from_iter([(
        "or".to_string(),
        JsonValue::Array(likes),
    )]));
    // AND the caller's filter with the free-text OR group.
    Ok(Map::from_iter([(
        "and".to_string(),
        JsonValue::Array(vec![JsonValue::Object(filter), text_clause]),
    )]))
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
pub async fn db_count(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::query")?;
    let req: CountRequest = parse_req(payload)?;
    let grant = require_model(ctx, &req.model, false)?;
    let Some((condition, binds, _)) =
        guarded_condition(ctx, grant, &req.model, &req.filter).await?
    else {
        return Ok(serde_json::json!({ "ok": true, "data": 0 }));
    };
    let mut body = String::from("LET $rows = SELECT count() AS n FROM type::table($__table)");
    if !condition.is_empty() {
        body.push_str(" WHERE ");
        body.push_str(&condition);
    }
    body.push_str(" GROUP ALL;");
    let hash = query_hash(
        &grant.table,
        &body,
        &binds,
        &std::collections::HashSet::new(),
    );
    if let Some(cached) = mantle_get_agg(ctx, &req.model, &grant.table, "count", &hash) {
        audit_cached(ctx, &req.model, &grant.table, &cached).await?;
        return Ok(serde_json::json!({ "ok": true, "data": cached.data }));
    }
    let flight_key = format!("{}|agg|{}|count|{hash}", plugin_cache_ns(ctx), req.model);
    let _flight = flight(ctx, &flight_key).await;
    if _flight.is_some()
        && let Some(cached) = mantle_get_agg(ctx, &req.model, &grant.table, "count", &hash)
    {
        audit_cached(ctx, &req.model, &grant.table, &cached).await?;
        return Ok(serde_json::json!({ "ok": true, "data": cached.data }));
    }
    let stamp_before = table_gen(ctx, &grant.table);
    let rows = run_audited(
        ctx,
        Access {
            operation: "read",
            model: req.model.clone(),
            table: &grant.table,
        },
        &body,
        "[]",
        binds,
        Extra::default(),
    )
    .await?;
    // No matching record leaves no group at all.
    let count = rows
        .first()
        .and_then(|row| row.get("n"))
        .and_then(JsonValue::as_u64)
        .unwrap_or(0);
    if let Some(stamp) = stamp_before {
        mantle_put_agg(
            ctx,
            &req.model,
            "count",
            &hash,
            stamp,
            &JsonValue::from(count),
            rows.len(),
        );
    }
    Ok(serde_json::json!({ "ok": true, "data": count }))
}

/// Grouped figures over the records that match a filter: counts, sums, averages, minimums and
/// maximums, per value of the `group_by` fields (or one row for the whole set).
pub async fn db_aggregate(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::query")?;
    let req: AggregateRequest = parse_req(payload)?;
    let grant = require_model(ctx, &req.model, false)?;
    let Some((condition, binds, hidden)) =
        guarded_condition(ctx, grant, &req.model, &req.filter).await?
    else {
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
    let hash = query_hash(&grant.table, &body, &binds, &hidden);
    if let Some(cached) = mantle_get_agg(ctx, &req.model, &grant.table, "aggregate", &hash) {
        audit_cached(ctx, &req.model, &grant.table, &cached).await?;
        return Ok(serde_json::json!({ "ok": true, "data": cached.data }));
    }
    let flight_key = format!("{}|agg|{}|aggregate|{hash}", plugin_cache_ns(ctx), req.model);
    let _flight = flight(ctx, &flight_key).await;
    if _flight.is_some()
        && let Some(cached) = mantle_get_agg(ctx, &req.model, &grant.table, "aggregate", &hash)
    {
        audit_cached(ctx, &req.model, &grant.table, &cached).await?;
        return Ok(serde_json::json!({ "ok": true, "data": cached.data }));
    }
    let stamp_before = table_gen(ctx, &grant.table);
    let rows = run_audited(
        ctx,
        Access {
            operation: "read",
            model: req.model.clone(),
            table: &grant.table,
        },
        &body,
        "[]",
        binds,
        Extra::default(),
    )
    .await?;
    let produced = rows.len();
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
    let data = JsonValue::Array(rows);
    if let Some(stamp) = stamp_before {
        mantle_put_agg(ctx, &req.model, "aggregate", &hash, stamp, &data, produced);
    }
    Ok(serde_json::json!({ "ok": true, "data": data }))
}

pub async fn db_find(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::query")?;
    let req: FindRequest = parse_req(payload)?;
    let mut reply = find_records(ctx, payload).await?;
    if !req.expand.is_empty() {
        let grant = require_model(ctx, &req.model, false)?;
        if let Some(schema) = &grant.schema {
            let mut rows: Vec<JsonValue> = reply
                .get("data")
                .and_then(JsonValue::as_array)
                .cloned()
                .unwrap_or_default();
            super::nested::expand(ctx, schema, &mut rows, &req.expand).await?;
            reply["data"] = JsonValue::Array(rows);
        }
    }
    Ok(reply)
}

/// Faceted / free-text list helper: expand `q` over `fields`, then find + count.
///
/// Returns `{ ok: true, data: { rows, total } }`.
pub async fn db_search(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::query")?;
    let req: SearchRequest = parse_req(payload)?;
    let filter = expand_search_filter(req.filter, req.q.as_deref(), &req.fields)?;
    let find_payload = serde_json::json!({
        "model": req.model,
        "filter": filter,
        "limit": req.limit,
        "offset": req.offset,
        "order": req.order,
        "expand": req.expand,
    });
    let count_payload = serde_json::json!({
        "model": find_payload["model"],
        "filter": find_payload["filter"],
    });
    let find_reply = db_find(ctx, &find_payload).await?;
    let count_reply = db_count(ctx, &count_payload).await?;
    let rows = find_reply
        .get("data")
        .cloned()
        .unwrap_or(JsonValue::Array(vec![]));
    let total = count_reply
        .get("data")
        .cloned()
        .unwrap_or(JsonValue::from(0));
    Ok(serde_json::json!({
        "ok": true,
        "data": { "rows": rows, "total": total }
    }))
}

/// `db::find` without the rows of child fields.
pub(super) async fn find_records(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::query")?;
    let req: FindRequest = parse_req(payload)?;
    let grant = require_model(ctx, &req.model, false)?;

    let Some((condition, mut binds, hidden)) =
        guarded_condition(ctx, grant, &req.model, &req.filter).await?
    else {
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
        // `id` is the record id, not a model field. Lists sort `-id` for newest first.
        let column = if name == "id" {
            "id".to_string()
        } else {
            match &grant.schema {
                Some(schema) => schema.column_id(name)?.to_string(),
                None => name.to_string(),
            }
        };
        validate_field_name(&column)?;
        body.push_str(&format!(" ORDER BY {column}{direction}"));
    }
    let limit = req
        .limit
        .map_or(MAX_ROWS_PER_FIND, |limit| limit.min(MAX_ROWS_PER_FIND));
    body.push_str(&format!(" LIMIT {limit}"));
    if let Some(offset) = req.offset {
        body.push_str(&format!(" START {offset}"));
    }
    body.push(';');

    let hash = query_hash(&grant.table, &body, &binds, &hidden);
    if let Some(cached) = mantle_get_list(ctx, &req.model, &grant.table, &hash) {
        audit_cached(ctx, &req.model, &grant.table, &cached).await?;
        return Ok(serde_json::json!({ "ok": true, "data": cached.data }));
    }
    let flight_key = format!("{}|list|{}|{hash}", plugin_cache_ns(ctx), req.model);
    let _flight = flight(ctx, &flight_key).await;
    if _flight.is_some()
        && let Some(cached) = mantle_get_list(ctx, &req.model, &grant.table, &hash)
    {
        audit_cached(ctx, &req.model, &grant.table, &cached).await?;
        return Ok(serde_json::json!({ "ok": true, "data": cached.data }));
    }
    let stamp_before = table_gen(ctx, &grant.table);

    let rows = run_audited(
        ctx,
        Access {
            operation: "read",
            model: req.model.clone(),
            table: &grant.table,
        },
        &body,
        "$rows.id",
        binds,
        Extra::default(),
    )
    .await?;
    let rows: Vec<JsonValue> = rows
        .into_iter()
        .map(|row| guard::strip(&hidden, decode(grant, row)))
        .collect();
    let ids = record_ids_of(&rows);
    let n = rows.len();
    let data = JsonValue::Array(rows);
    if let Some(stamp) = stamp_before {
        mantle_put_list(ctx, &req.model, &hash, stamp, &data, &ids, n);
    }
    Ok(serde_json::json!({ "ok": true, "data": data }))
}

pub async fn db_create(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::mutate")?;
    let req: CreateRequest = parse_req(payload)?;
    if let Some(reply) = super::nested::create(ctx, &req).await? {
        return Ok(reply);
    }
    run_plan(
        ctx,
        plan_create_with(ctx, req.model, req.data, Options::default()).await?,
    )
    .await
}

/// Create the record, or update the one that has these `key` values. Payload:
/// `{ "model", "key": { field: value, … }, "data": { … } }`. The key fields are written on create
/// (and never changed on update). The write is an ordinary `db::create` or `db::update`, so rules,
/// checks, audit and chatter all apply; two callers racing to create the same key are stopped by
/// a unique index (or `unique_when`) on it, as for any create. More than one record with the key
/// is refused rather than guessed. The reply is the write's, with `created: true | false`.
pub async fn db_upsert(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::mutate")?;
    ctx.require_cap("db::query")?;
    let model = payload
        .get("model")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| HostError::InvalidPayload("`model` is required".into()))?;
    let key = payload
        .get("key")
        .and_then(JsonValue::as_object)
        .filter(|k| !k.is_empty() && k.len() <= 8)
        .ok_or_else(|| HostError::InvalidPayload("`key` is 1 to 8 fields".into()))?;
    let data = match payload.get("data") {
        Some(JsonValue::Object(map)) => map.clone(),
        None | Some(JsonValue::Null) => Map::new(),
        _ => return Err(HostError::InvalidPayload("`data` must be an object".into())),
    };
    let found = find_records(
        ctx,
        &serde_json::json!({ "model": model, "filter": key, "limit": 2 }),
    )
    .await?;
    let rows = found
        .get("data")
        .and_then(JsonValue::as_array)
        .cloned()
        .unwrap_or_default();
    let mut reply = match rows.as_slice() {
        [] => {
            let mut full = data;
            for (name, value) in key {
                full.insert(name.clone(), value.clone());
            }
            let mut reply =
                db_create(ctx, &serde_json::json!({ "model": model, "data": full })).await?;
            reply["created"] = JsonValue::Bool(true);
            reply
        }
        [row] => {
            let id = record_id_of(row)
                .ok_or_else(|| HostError::InvalidPayload("the record has no id".into()))?
                .to_string();
            let mut changes = data;
            for name in key.keys() {
                changes.remove(name);
            }
            let mut reply = if changes.is_empty() {
                serde_json::json!({ "ok": true, "data": row })
            } else {
                db_update(ctx, &serde_json::json!({ "model": model, "id": id, "data": changes })).await?
            };
            reply["created"] = JsonValue::Bool(false);
            reply
        }
        _ => {
            return Err(HostError::InvalidPayload(format!(
                "more than one `{model}` record has that key; it is not unique"
            )))
        }
    };
    if reply.get("ok").is_none() {
        reply["ok"] = JsonValue::Bool(true);
    }
    Ok(reply)
}

async fn plan_create<'a>(
    ctx: &'a PluginHostContext,
    payload: &JsonValue,
) -> Result<Plan<'a>, HostError> {
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
    let scope = guard::scope(ctx, grant, Operation::Create, "d8")
        .await?
        .ok_or_else(|| guard::denied(&model, "create"))?;
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
        (
            "__table".to_string(),
            JsonValue::String(grant.table.clone()),
        ),
        ("__data".to_string(), JsonValue::Object(data.clone())),
    ];
    let mut extra = chatter_extra(grant, "create")?;
    let mut content = "$__data".to_string();
    if let Some(schema) = &grant.schema {
        super::graph::create_extra(schema, &data, &mut binds, &mut extra);
        super::graph::link_checks(schema, &data, &mut binds, &mut extra);
        let injected = options
            .inject
            .as_ref()
            .map(|i| (i.column.as_str(), i.value_sql.as_str()));
        content =
            super::integrity::create_content(schema, &data, injected, &mut binds, &mut extra)?;
        super::integrity::derived_fields(schema, &mut extra)?;
        if !options.defer_checks {
            super::integrity::model_checks(schema, "$rows", &mut binds, &mut extra)?;
        }
        if !options.skip_parent_refresh {
            super::integrity::refresh_parents(schema, &mut extra)?;
        }
    }
    if let (Some((field, states)), Some(schema)) =
        (guard::workflow_initial(ctx, grant).await?, &grant.schema)
    {
        let column = schema.column_id(&field)?;
        validate_field_name(column)?;
        extra.after.push(format!(
            "IF array::len((SELECT VALUE id FROM $rows[0].id WHERE {column} IN $d30010)) = 0 {{ THROW $d30011; }};"
        ));
        binds.push((
            "d30011".to_string(),
            JsonValue::String(format!(
                "aether-denied: a new `{model}` record starts as {}",
                states.join(" or ")
            )),
        ));
        binds.push((
            "d30010".to_string(),
            JsonValue::Array(states.into_iter().map(JsonValue::String).collect()),
        ));
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
        access: Access {
            operation: "create",
            model: model,
            table: &grant.table,
        },
        grant,
        body: format!("LET $rows = (CREATE type::table($__table) CONTENT {content} RETURN AFTER);"),
        ids: "$rows.id".into(),
        binds,
        extra,
    })
}

pub async fn db_update(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::mutate")?;
    let req: UpdateRequest = parse_req(payload)?;
    if let Some(reply) = super::nested::update(ctx, &req).await? {
        bust_model_row(ctx, &req.model, &req.id);
        return Ok(reply);
    }
    let model = req.model.clone();
    let id = req.id.clone();
    let out = run_plan(
        ctx,
        plan_update_with(ctx, req.model, req.id, req.data, Options::default()).await?,
    )
    .await?;
    // The returned row id and the id the caller passed can differ (`key` vs `table:key`).
    bust_model_row(ctx, &model, &id);
    Ok(out)
}

async fn plan_update<'a>(
    ctx: &'a PluginHostContext,
    payload: &JsonValue,
) -> Result<Plan<'a>, HostError> {
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
    let scope = guard::scope(ctx, grant, Operation::Write, "d8")
        .await?
        .ok_or_else(|| guard::denied(&model, "change"))?;
    let mut binds = record_binds(grant, &id);
    let mut extra = chatter_extra(grant, "update")?;
    add_record_check(&mut extra, &mut binds, scope, &model, "change");
    let body = match &grant.schema {
        Some(schema) => {
            // One statement that sets the changed columns (by id) and clears the ones set to null.
            let (set, clear) = schema.encode_update(&data)?;
            workflow_move(
                ctx, grant, schema, &set, &clear, &model, &mut binds, &mut extra,
            )
            .await?;
            super::graph::update_extra(schema, &set, &clear, &mut binds, &mut extra);
            super::graph::link_checks(schema, &set, &mut binds, &mut extra);
            super::integrity::derived_fields(schema, &mut extra)?;
            if !options.defer_checks {
                super::integrity::model_checks(schema, "$rows", &mut binds, &mut extra)?;
            }
            if !options.skip_parent_refresh {
                super::integrity::refresh_parents(schema, &mut extra)?;
            }
            if !schema.dependents.is_empty() {
                let touched: Vec<&str> = set
                    .iter()
                    .map(|(column, _)| column.as_str())
                    .chain(clear.iter().map(|c| c.as_str()))
                    .collect();
                super::integrity::propagate_copies(schema, &touched, &mut extra)?;
            }
            let mut assignments =
                super::integrity::assign_on_update(schema, &set, &mut binds, &mut extra)?;
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
            "LET $rows = (UPDATE type::record($__table, $__key) MERGE $__data RETURN AFTER);"
                .to_string()
        }
    };
    Ok(Plan {
        access: Access {
            operation: "update",
            model: model,
            table: &grant.table,
        },
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
    let Some(workflow) = grant
        .rules
        .as_ref()
        .and_then(|rules| rules.workflow.as_ref())
    else {
        return Ok(());
    };
    let column = schema.column_id(&workflow.field)?.to_string();
    let refuse =
        |to: &str| HostError::Denied(format!("you may not move this `{model}` record to `{to}`"));
    if clear.contains(&column) {
        return match guard::workflow_scope(ctx, grant, "", "d9").await? {
            Some(scope) if scope.sql.is_empty() => Ok(()),
            _ => Err(refuse("(nothing)")),
        };
    }
    let Some(to) = set.get(&column).and_then(JsonValue::as_str) else {
        return Ok(());
    };
    let Some(scope) = guard::workflow_scope(ctx, grant, to, "d9").await? else {
        return Err(refuse(to));
    };
    if scope.sql.is_empty() {
        return Ok(());
    }
    extra.before.push(format!(
        "IF array::len((SELECT VALUE id FROM type::record($__table, $__key) WHERE {})) = 0 {{ THROW $d30012; }};",
        scope.sql
    ));
    binds.extend(scope.binds);
    binds.push((
        "d30012".to_string(),
        JsonValue::String(format!(
            "aether-denied: you may not move this `{model}` record to `{to}` from its state now"
        )),
    ));
    Ok(())
}

/// Add to a number field in place. The addition happens in the database, in one statement, so two
/// callers incrementing at once both count: use it for counters and sequences, never read, add and
/// write back.
pub async fn db_increment(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    run_plan(ctx, plan_increment(ctx, payload).await?).await
}

async fn plan_increment<'a>(
    ctx: &'a PluginHostContext,
    payload: &JsonValue,
) -> Result<Plan<'a>, HostError> {
    ctx.require_cap("db::mutate")?;
    let req: IncrementRequest = parse_req(payload)?;
    let grant = require_model(ctx, &req.model, true)?;
    let (column, by) = match &grant.schema {
        Some(schema) => (
            schema.numeric_column(&req.field)?.to_string(),
            schema.increment_by(&req.field, &req.by)?,
        ),
        None => (req.field.clone(), req.by.clone()),
    };
    let number = by.as_f64().filter(|n| n.is_finite());
    let Some(number) = number else {
        return Err(HostError::InvalidPayload(
            "`by` must be a finite number".into(),
        ));
    };
    validate_field_name(&column)?;
    let (_, locked) = guard::field_limits(ctx, grant).await?;
    guard::check_locked(&locked, [&req.field], &req.model)?;
    let scope = guard::scope(ctx, grant, Operation::Write, "d8")
        .await?
        .ok_or_else(|| guard::denied(&req.model, "change"))?;
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
        access: Access {
            operation: "update",
            model: req.model,
            table: &grant.table,
        },
        grant,
        body,
        ids: RECORD_ID.into(),
        binds,
        extra,
    })
}

pub async fn db_delete(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::mutate")?;
    let req: DeleteRequest = parse_req(payload)?;
    if let Some(reply) = super::nested::delete(ctx, &req).await? {
        bust_model_row(ctx, &req.model, &req.id);
        return Ok(reply);
    }
    let model = req.model.clone();
    let id = req.id.clone();
    let out = run_plan(
        ctx,
        plan_delete_with(ctx, req.model, req.id, Options::default()).await?,
    )
    .await?;
    bust_model_row(ctx, &model, &id);
    Ok(out)
}

async fn plan_delete<'a>(
    ctx: &'a PluginHostContext,
    payload: &JsonValue,
) -> Result<Plan<'a>, HostError> {
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
    let scope = guard::scope(ctx, grant, Operation::Delete, "d8")
        .await?
        .ok_or_else(|| guard::denied(&model, "delete"))?;
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
        access: Access {
            operation: "delete",
            model: model,
            table: &grant.table,
        },
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
const PER_WRITE_VARIABLES: &[&str] = &[
    "rows",
    "__table",
    "__key",
    "__data",
    "__by",
    "__before",
    "__changed",
    "__audit_model",
    "__audit_table",
    "__audit_operation",
];

fn is_per_write(name: &str) -> bool {
    PER_WRITE_VARIABLES.contains(&name)
        || (name.len() > 1
            && name.starts_with('d')
            && name[1..].bytes().all(|byte| byte.is_ascii_digit()))
}

/// `text` with every per-write variable (`$rows`, `$__table`, `$d0`, …) renamed by `suffix`.
fn rename_variables(text: &str, suffix: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut rest = text;
    while let Some(dollar) = rest.find('$') {
        out.push_str(&rest[..=dollar]);
        rest = &rest[dollar + 1..];
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
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
pub async fn db_transaction(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
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
            HostError::InvalidPayload(message) => {
                HostError::InvalidPayload(format!("write {index}: {message}"))
            }
            other => other,
        })?;
        plans.push(plan);
    }

    let data = run_composed(ctx, &plans).await?;
    Ok(serde_json::json!({ "ok": true, "data": data }))
}

/// Run several writes in one transaction, each with its own copy of the per-write variables, and
/// answer each write's record (or null), decoded as the plugin sees it.
pub(super) async fn run_composed(
    ctx: &PluginHostContext,
    plans: &[Plan<'_>],
) -> Result<Vec<JsonValue>, HostError> {
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
            let name = if is_per_write(name) {
                format!("{name}{suffix}")
            } else {
                name.clone()
            };
            binds.push((name, value.clone()));
        }
        binds.push((
            format!("__audit_model{suffix}"),
            JsonValue::String(plan.access.model.clone()),
        ));
        binds.push((
            format!("__audit_table{suffix}"),
            JsonValue::String(plan.access.table.to_string()),
        ));
        binds.push((
            format!("__audit_operation{suffix}"),
            JsonValue::String(plan.access.operation.to_string()),
        ));
    }
    statements.push_str(&format!(
        "RETURN [{}];\nCOMMIT TRANSACTION;",
        returned.join(", ")
    ));

    let mut query = ctx
        .db
        .query(statements)
        .bind(("__audit_request", ctx.audit.request_id.clone()))
        .bind(("__audit_actor_type", ctx.audit.actor.kind().to_string()))
        .bind(("__audit_actor_id", ctx.audit.actor.id().map(str::to_string)))
        .bind(("__audit_plugin", ctx.plugin_name.clone()))
        .bind(("__audit_function", ctx.function.clone()))
        .bind(("__audit_ip", ctx.audit.ip.clone()))
        .bind((
            "__cm_author",
            ctx.audit.actor.id().unwrap_or("system").to_string(),
        ));
    for (name, value) in binds {
        query = query.bind((name, value));
    }
    let mut response = checked(query.await.map_err(map_db_error)?)?;
    let results: Vec<Vec<JsonValue>> = response.take(statement_count)?;
    let data: Vec<JsonValue> = results
        .into_iter()
        .zip(plans)
        .zip(&hiddens)
        .map(|((rows, plan), hidden)| {
            rows.into_iter().next().map_or(JsonValue::Null, |row| {
                guard::strip(hidden, decode(plan.grant, row))
            })
        })
        .collect();
    for (plan, row) in plans.iter().zip(&data) {
        if let Some(op) = mutate_op(plan.access.operation) {
            let id = if row.is_null() {
                None
            } else {
                record_id_of(row)
            };
            publish_data_changed(ctx, &plan.access.model, op, id);
            mantle_bust_row(ctx, &plan.access.model, plan.access.table, id);
        }
    }
    Ok(data)
}

fn bust_model_row(ctx: &PluginHostContext, model: &str, id: &str) {
    let Ok(grant) = require_model(ctx, model, true) else {
        return;
    };
    let table = grant.table.clone();
    mantle_bust_row(ctx, model, &table, Some(id));
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
        assert!(renamed.contains(
            "LET $rows_3 = (UPDATE type::record($__table_3, $__key_3) SET a = $d0_3, b = $d12_3"
        ));
        assert!(renamed.contains("$rows_3[0].id"));
        assert!(
            renamed.contains("$__before_3[$k]"),
            "lambda variables are not renamed"
        );
        for shared in ["$__cm_author;", "$__audit_plugin;", "$d;", "$data"] {
            assert!(
                renamed.contains(shared),
                "{shared} must stay as it is: {renamed}"
            );
        }
    }

    #[test]
    fn row_cache_id_is_the_same_for_a_bare_key_and_a_record_id() {
        let table = "mdl_employee";
        let bare = "422r544m9rghrhuwbgya";
        let full = format!("{table}:{bare}");
        assert_eq!(row_cache_id(table, bare), full);
        assert_eq!(row_cache_id(table, &full), full);
        let keys = row_cache_keys(table, &full);
        assert!(keys.contains(&full));
        assert!(keys.contains(&bare.to_string()));
    }

    #[test]
    fn field_and_table_names_must_be_plain_identifiers() {
        for good in ["body", "created_at", "chat_message", "A1"] {
            assert!(validate_ident(good).is_ok(), "{good}");
        }
        for bad in [
            "",
            "a b",
            "a;b",
            "a.b",
            "a-b",
            "x) OR true --",
            "type::table",
        ] {
            assert!(validate_ident(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn empty_q_leaves_the_filter_alone() {
        let filter = Map::from_iter([("status".into(), JsonValue::String("active".into()))]);
        let fields = vec!["name".into()];
        let expanded = expand_search_filter(filter.clone(), Some("  "), &fields).unwrap();
        assert_eq!(expanded, filter);
        let expanded = expand_search_filter(filter.clone(), None, &fields).unwrap();
        assert_eq!(expanded, filter);
    }

    #[test]
    fn q_without_fields_is_rejected() {
        let err = expand_search_filter(Map::new(), Some("ann"), &[]).unwrap_err();
        assert!(matches!(err, HostError::InvalidPayload(_)));
    }

    #[test]
    fn q_becomes_an_or_of_likes_anded_with_the_caller_filter() {
        let filter = Map::from_iter([("status".into(), JsonValue::String("active".into()))]);
        let fields = vec!["name".into(), "email".into()];
        let expanded = expand_search_filter(filter, Some("ann"), &fields).unwrap();
        assert_eq!(
            JsonValue::Object(expanded),
            serde_json::json!({
                "and": [
                    { "status": "active" },
                    { "or": [
                        { "name": { "like": "ann" } },
                        { "email": { "like": "ann" } }
                    ] }
                ]
            })
        );
    }

    #[tokio::test]
    async fn publish_data_changed_reaches_org_subscribers() {
        use crate::kernel::host::test_support::dummy_ctx;
        use crate::notifications::HubMessage;

        let mut ctx = dummy_ctx(&[]);
        ctx.database = "org_acme".into();
        let mut receiver = ctx.notifications.subscribe("org_acme");

        publish_data_changed(&ctx, "employee", "create", Some("employee:1"));

        let HubMessage::Event(event) = receiver.recv().await.unwrap() else {
            panic!("expected a transient event");
        };
        assert_eq!(event.plugin, "aether");
        assert_eq!(event.event, "data.changed");
        assert_eq!(event.payload["model"], "employee");
        assert_eq!(event.payload["op"], "create");
        assert_eq!(event.payload["id"], "employee:1");
    }

    #[test]
    fn mutate_op_maps_writes_only() {
        assert_eq!(mutate_op("create"), Some("create"));
        assert_eq!(mutate_op("update"), Some("update"));
        assert_eq!(mutate_op("delete"), Some("delete"));
        assert_eq!(mutate_op("read"), None);
    }
}
