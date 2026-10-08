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

/// The `via:` markers a call trail earns: `via:<plugin>` for each other plugin whose function led to this
/// call (and `via:*` when there is one), and `via:self` when one of the plugin's own functions did. A direct
/// call to the record API, with no function of the plugin in between, earns none of them.
pub fn trail_markers(plugin_name: &str, trail: &[String]) -> Vec<String> {
    let mut held: Vec<String> = Vec::new();
    let mut through_others = false;
    for step in trail {
        let plugin = step.split('.').next().unwrap_or_default();
        if plugin.is_empty() {
            continue;
        }
        let marker = if plugin == plugin_name {
            "via:self".to_string()
        } else {
            through_others = true;
            format!("via:{plugin}")
        };
        if !held.contains(&marker) {
            held.push(marker);
        }
    }
    if through_others {
        held.push("via:*".to_string());
    }
    held
}

/// The caller's roles, plus the `via:` markers of the call trail: a rule may trust what another plugin's
/// functions do for a caller (`via:<plugin>`, `via:*`), or insist that a field is only changed by the plugin's
/// own functions (`via:self`), which is how a ledger keeps its counters and closed periods out of reach of
/// the generic record API.
pub async fn held_roles(ctx: &PluginHostContext) -> Result<Vec<String>, HostError> {
    let mut held = roles(ctx).await?.clone();
    for marker in trail_markers(&ctx.plugin_name, &ctx.call_trail) {
        if !held.contains(&marker) {
            held.push(marker);
        }
    }
    Ok(held)
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
    let held = &held_roles(ctx).await?;
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

/// What the workflow of `grant`'s model allows the caller to do when a write sets the workflow
/// field to `to`. `None`: no transition into `to` is open to them. An empty condition: nothing
/// limits it (no workflow, or the caller is exempt). Otherwise the condition the record must meet
/// before the write: already in `to`, or in a state a transition they may make leaves.
pub async fn workflow_scope(ctx: &PluginHostContext, grant: &ModelGrant, to: &str, prefix: &str) -> Result<Option<Scope>, HostError> {
    let free = || Ok(Some(Scope { sql: String::new(), binds: Vec::new() }));
    let Some(rules) = &grant.rules else { return free() };
    if rules.workflow.is_none() || exempt(ctx).await? {
        return free();
    }
    let held = &held_roles(ctx).await?;
    let mut values = HashMap::new();
    for name in rules.workflow_variables(&ctx.plugin_name, held) {
        let value = variable(ctx, &name).await?;
        values.insert(name, value);
    }
    match rules.move_to(&ctx.plugin_name, to, held, &values)? {
        Decision::Deny => Ok(None),
        Decision::Open => free(),
        Decision::Only(filter) => {
            let Compiled { sql, binds } = match &grant.schema {
                Some(schema) => compile_filter(&filter, schema.as_ref(), prefix)?,
                None => compile_filter(&filter, &Unchecked, prefix)?,
            };
            Ok(Some(Scope { sql, binds }))
        }
    }
}

/// The workflow field of the model and the states a new record may start in, when the caller is
/// held to the workflow. `None` when they are not.
pub async fn workflow_initial(ctx: &PluginHostContext, grant: &ModelGrant) -> Result<Option<(String, Vec<String>)>, HostError> {
    let Some(rules) = &grant.rules else { return Ok(None) };
    let Some(workflow) = &rules.workflow else { return Ok(None) };
    if exempt(ctx).await? {
        return Ok(None);
    }
    let states = if workflow.initial.is_empty() {
        grant.schema.as_ref().and_then(|schema| schema.default_text(&workflow.field)).into_iter().collect()
    } else {
        workflow.initial.clone()
    };
    Ok(Some((workflow.field.clone(), states)))
}

/// A move the caller may make, with the condition a record must meet (SurrealQL over the model's
/// columns).
pub struct Move {
    pub name: String,
    pub label: Option<String>,
    pub from: Vec<String>,
    pub to: String,
    pub sql: String,
    pub binds: Vec<(String, JsonValue)>,
}

/// The transitions of the model's workflow the caller may make. An exempt caller may make any
/// from its states. Bind names start with `prefix`.
pub async fn workflow_moves(ctx: &PluginHostContext, grant: &ModelGrant, prefix: &str) -> Result<Vec<Move>, HostError> {
    let Some(rules) = &grant.rules else { return Ok(Vec::new()) };
    let Some(workflow) = &rules.workflow else { return Ok(Vec::new()) };
    let compile = |filter: &crate::data_model::Filter, prefix: &str| -> Result<Compiled, HostError> {
        Ok(match &grant.schema {
            Some(schema) => compile_filter(filter, schema.as_ref(), prefix)?,
            None => compile_filter(filter, &Unchecked, prefix)?,
        })
    };
    let mut out = Vec::new();
    if exempt(ctx).await? {
        for (index, transition) in workflow.transitions.iter().enumerate() {
            let from = crate::data_model::Filter::Cmp {
                field: workflow.field.clone(),
                op: crate::data_model::query::Op::In,
                value: JsonValue::Array(transition.from.iter().cloned().map(JsonValue::String).collect()),
            };
            let Compiled { sql, binds } = compile(&from, &format!("{prefix}{index}x"))?;
            out.push(Move { name: transition.name.clone(), label: transition.label.clone(), from: transition.from.clone(), to: transition.to.clone(), sql, binds });
        }
        return Ok(out);
    }
    let held = &held_roles(ctx).await?;
    let mut values = HashMap::new();
    for name in rules.workflow_variables(&ctx.plugin_name, held) {
        let value = variable(ctx, &name).await?;
        values.insert(name, value);
    }
    for (index, (transition, filter)) in rules.available_moves(&ctx.plugin_name, held, &values)?.into_iter().enumerate() {
        let Compiled { sql, binds } = compile(&filter, &format!("{prefix}{index}x"))?;
        out.push(Move { name: transition.name.clone(), label: transition.label.clone(), from: transition.from.clone(), to: transition.to.clone(), sql, binds });
    }
    Ok(out)
}

/// The fields the caller may not read and may not set, by field name.
pub async fn field_limits(ctx: &PluginHostContext, grant: &ModelGrant) -> Result<(HashSet<String>, HashSet<String>), HostError> {
    let Some(rules) = &grant.rules else { return Ok(Default::default()) };
    if exempt(ctx).await? {
        return Ok(Default::default());
    }
    let held = &held_roles(ctx).await?;
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

#[cfg(test)]
mod trail_tests {
    use super::trail_markers;

    fn trail(steps: &[&str]) -> Vec<String> {
        steps.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_direct_call_earns_no_marker() {
        assert!(trail_markers("gl", &[]).is_empty());
    }

    #[test]
    fn the_plugins_own_function_earns_via_self_only() {
        assert_eq!(trail_markers("gl", &trail(&["gl.post_entry"])), vec!["via:self"]);
    }

    #[test]
    fn another_plugin_in_the_chain_earns_its_marker_and_the_wildcard() {
        let held = trail_markers("hr", &trail(&["hr_onboarding.start", "hr.hire", "hr.hire"]));
        assert_eq!(held, vec!["via:hr_onboarding", "via:self", "via:*"]);
    }
}
