//! `db::transitions`: the moves a person may make with a record right now.

use serde_json::{Value as JsonValue, json};

use super::context::PluginHostContext;
use super::db::{Access, Extra, GetRequest, RECORD_ID, parse_req, record_binds, require_model, run_audited};
use super::error::HostError;
use super::guard;
use crate::data_model::Operation;

/// The transitions of the model's workflow that the caller may make with this record in the
/// state it is in: each is `{ name, label, from, to }`. The record must be one they may read and
/// change; otherwise (or with no workflow) the list is empty. A page uses it to show only the
/// buttons that will work. The move itself is still checked when it is made.
pub async fn db_transitions(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("db::query")?;
    let req: GetRequest = parse_req(payload)?;
    let grant = require_model(ctx, &req.model, false)?;
    let none = || Ok(json!({ "ok": true, "data": [] }));
    let moves = guard::workflow_moves(ctx, grant, "m").await?;
    if moves.is_empty() {
        return none();
    }
    let Some(read) = guard::scope(ctx, grant, Operation::Read, "r").await? else { return none() };
    let Some(write) = guard::scope(ctx, grant, Operation::Write, "w").await? else { return none() };
    let mut binds = record_binds(grant, &req.id);
    let mut columns = Vec::new();
    for (index, mv) in moves.iter().enumerate() {
        columns.push(format!("({}) AS t{index}", mv.sql));
    }
    binds.extend(moves.iter().flat_map(|mv| mv.binds.clone()));
    let mut conditions = Vec::new();
    for scope in [read, write] {
        if !scope.sql.is_empty() {
            conditions.push(format!("({})", scope.sql));
            binds.extend(scope.binds);
        }
    }
    let filter = if conditions.is_empty() { String::new() } else { format!(" WHERE {}", conditions.join(" AND ")) };
    let body = format!("LET $rows = SELECT id, {} FROM type::record($__table, $__key){filter};", columns.join(", "));
    let rows = run_audited(
        ctx,
        Access { operation: "read", model: req.model.clone(), table: &grant.table },
        &body,
        RECORD_ID,
        binds,
        Extra::default(),
    )
    .await?;
    let Some(row) = rows.first() else { return none() };
    let open: Vec<JsonValue> = moves
        .iter()
        .enumerate()
        .filter(|(index, _)| row.get(format!("t{index}")).and_then(JsonValue::as_bool) == Some(true))
        .map(|(_, mv)| json!({ "name": mv.name, "label": mv.label.clone().unwrap_or_else(|| mv.name.clone()), "from": mv.from, "to": mv.to }))
        .collect();
    Ok(json!({ "ok": true, "data": open }))
}
