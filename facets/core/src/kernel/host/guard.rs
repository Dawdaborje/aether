//! Applying a plugin's record and field rules (`rules/<model>.json`, see
//! [`crate::data_model::rules`]) to the database commands.
//!
//! For each command the guard answers one question: which records may this caller read, change,
//! create or delete? The answer becomes a `WHERE` condition added to the statement, or a check that
//! runs in the same transaction as the write and aborts it, so a refused write changes nothing.
//! Administrators (`org_admin`), the kernel's own jobs and the small `rule_var_*` functions that
//! answer a rule's variables are not held to the rules.

use std::collections::{HashMap, HashSet};

use serde_json::Value as JsonValue;

use super::context::{ModelGrant, PluginHostContext};
use super::error::HostError;
use crate::data_model::query::{compile_filter, Compiled, Unchecked};
use crate::data_model::{Decision, Operation};

/// A function named `rule_var_<name>` answers the variable `$<plugin>.<name>` for the caller.
pub const VARIABLE_PREFIX: &str = "rule_var_";

/// The caller's roles in this organization, read once per call. Visitors and the kernel hold none.
pub async fn roles(ctx: &PluginHostContext) -> Result<&Vec<String>, HostError> {
    ctx.roles_cache
        .get_or_try_init(|| async {
            match ctx.audit.actor.id() {
                Some(user) if ctx.audit.actor.kind() == "user" => {
                    crate::roles::roles_of(&ctx.db, user).await.map_err(HostError::Db)
                }
                _ => Ok(Vec::new()),
            }
        })
        .await
}

/// Whether rules are set aside for this caller.
async fn exempt(ctx: &PluginHostContext) -> Result<bool, HostError> {
    if ctx.bypass_rules {
        return Ok(true);
    }
    Ok(roles(ctx).await?.iter().any(|role| role == crate::roles::ORG_ADMIN))
}

/// The value of a variable for the caller: `$user` is their account; `$<plugin>.<name>` is the
/// answer of that plugin's `rule_var_<name>` function (a bare `$<name>` asks the plugin itself).
async fn variable(ctx: &PluginHostContext, name: &str) -> Result<JsonValue, HostError> {
    if name == "user" {
        return Ok(ctx.audit.actor.id().map_or(JsonValue::Null, |id| JsonValue::String(id.to_string())));
    }
    let (plugin, short) = match name.split_once('.') {
        Some((plugin, short)) => (plugin.to_string(), short.to_string()),
        None => (ctx.plugin_name.clone(), name.to_string()),
    };
    let key = format!("{plugin}.{short}");
    if let Some(cached) = ctx.rule_cache.get(&key) {
        return Ok(cached);
    }
    if plugin != ctx.plugin_name && !ctx.dependencies.iter().any(|dependency| dependency == &plugin) {
        return Err(HostError::Message(format!(
            "a rule uses `${key}`, but `{plugin}` is not one of this plugin's dependencies"
        )));
    }
    let function = format!("{VARIABLE_PREFIX}{short}");
    let target = format!("{plugin}.{function}");
    if ctx.call_trail.len() >= super::plugin_call::MAX_DEPTH {
        return Err(HostError::Message("plugin calls are nested too deep to work out a rule".into()));
    }
    let caller = ctx
        .caller
        .as_ref()
        .ok_or_else(|| HostError::Message("this call cannot work out a rule's variable".into()))?;
    let mut trail = ctx.call_trail.clone();
    trail.push(target);
    let value = caller.call(&plugin, &function, serde_json::json!({}), trail).await?;
    ctx.rule_cache.set(&key, value.clone());
    Ok(value)
}

/// What the rules allow the caller to do to `grant`'s model with `op`.
pub async fn decide(ctx: &PluginHostContext, grant: &ModelGrant, op: Operation) -> Result<Decision, HostError> {
    let Some(rules) = &grant.rules else { return Ok(Decision::Open) };
    if !rules.limits(op) || exempt(ctx).await? {
        return Ok(Decision::Open);
    }
    let held = roles(ctx).await?;
    let mut values = HashMap::new();
    for name in rules.variables_needed(&ctx.plugin_name, op, held) {
        let value = variable(ctx, &name).await?;
        values.insert(name, value);
    }
    Ok(rules.decide(&ctx.plugin_name, op, held, &values)?)
}

/// A condition to add to a statement, or nothing when the caller may not touch any record.
pub struct Scope {
    /// Empty when every record is allowed.
    pub sql: String,
    pub binds: Vec<(String, JsonValue)>,
}

/// The condition for `op` on `grant`'s model, with bind names starting with `prefix`; `None` when
/// the rules allow the caller none of the records.
pub async fn scope(ctx: &PluginHostContext, grant: &ModelGrant, op: Operation, prefix: &str) -> Result<Option<Scope>, HostError> {
    match decide(ctx, grant, op).await? {
        Decision::Deny => Ok(None),
        Decision::Open => Ok(Some(Scope { sql: String::new(), binds: Vec::new() })),
        Decision::Only(filter) => {
            let Compiled { sql, binds } = match &grant.schema {
                Some(schema) => compile_filter(&filter, schema.as_ref(), prefix)?,
                None => compile_filter(&filter, &Unchecked, prefix)?,
            };
            Ok(Some(Scope { sql, binds }))
        }
    }
}

/// The fields the caller may not read and may not set, by field name.
pub async fn field_limits(ctx: &PluginHostContext, grant: &ModelGrant) -> Result<(HashSet<String>, HashSet<String>), HostError> {
    let Some(rules) = &grant.rules else { return Ok(Default::default()) };
    if exempt(ctx).await? {
        return Ok(Default::default());
    }
    let held = roles(ctx).await?;
    Ok((
        rules.hidden_fields(&ctx.plugin_name, held).into_iter().map(str::to_string).collect(),
        rules.locked_fields(&ctx.plugin_name, held).into_iter().map(str::to_string).collect(),
    ))
}

/// Refuse a write that sets a field the caller's roles do not allow them to set.
pub fn check_locked<'a>(locked: &HashSet<String>, fields: impl IntoIterator<Item = &'a String>, model: &str) -> Result<(), HostError> {
    let mut refused: Vec<&String> = fields.into_iter().filter(|field| locked.contains(*field)).collect();
    refused.sort();
    match refused.first() {
        Some(field) => Err(HostError::Denied(format!("you cannot set `{model}.{field}`"))),
        None => Ok(()),
    }
}

/// Refuse a search or summary that looks at a field the caller may not read: it would show what
/// the field holds even though the field itself is hidden.
pub fn check_unreadable<'a>(hidden: &HashSet<String>, fields: impl IntoIterator<Item = &'a str>, model: &str) -> Result<(), HostError> {
    match fields.into_iter().find(|field| hidden.contains(*field)) {
        Some(field) => Err(HostError::Denied(format!("you cannot search or summarise `{model}.{field}`"))),
        None => Ok(()),
    }
}

/// A record without the fields the caller may not read.
pub fn strip(hidden: &HashSet<String>, mut record: JsonValue) -> JsonValue {
    if hidden.is_empty() {
        return record;
    }
    if let Some(map) = record.as_object_mut() {
        map.retain(|key, _| !hidden.contains(key));
    }
    record
}

pub fn denied(model: &str, what: &str) -> HostError {
    HostError::Denied(format!("you may not {what} `{model}` records here"))
}
