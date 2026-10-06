//! Making an organization's database match a plugin's models.
//!
//! [`plan`] compares a model's definition with what was last applied in the organization (the
//! snapshot kept in its `model_schema` table) and says what has to change. Because values are
//! stored under field ids, most edits need nothing at all:
//!
//! | Edit | What happens to the database |
//! |---|---|
//! | rename a field or model, change a label, reorder, change the view | nothing moves (a rename refreshes the column's readable comment) |
//! | add a field | the column is defined; records are untouched |
//! | add a *required* field, or make one required, when records exist | needs a `default`, written into the records that lack a value |
//! | remove a field | it is **hidden**: the column and its data stay, plugins no longer see it |
//! | widen a type (int to float, string to text, select to string, ...) | the column is redefined |
//! | any other type change | blocked, unless the table is empty: it needs a migration |
//!
//! The first problem found blocks the whole upgrade before anything is changed.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use surrealdb::{Surreal, engine::remote::ws::Client};
use thiserror::Error;

use super::definition::{FieldDef, FieldType, IndexKind, ModelDef};

#[derive(Debug, Error)]
pub enum ApplyError {
    #[error("database error: {0}")]
    Database(#[from] surrealdb::Error),
    #[error("model `{model}` cannot be applied: {problems}", problems = .problems.join("; "))]
    Blocked { model: String, problems: Vec<String> },
    #[error("the applied schema of `{model}` could not be read: {source}")]
    Snapshot {
        model: String,
        #[source]
        source: serde_json::Error,
    },
}

/// A field as it was last applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedField {
    pub name: String,
    pub kind: FieldType,
    pub required: bool,
    pub index: Option<IndexKind>,
    /// Hidden: the column still exists.
    pub deprecated: bool,
    /// For a decimal: digits after the point. Older snapshots have none.
    #[serde(default)]
    pub scale: Option<u32>,
    /// A parent link kept as graph edges. Older snapshots have none.
    #[serde(default)]
    pub hierarchy: bool,
}

/// A multi-field index as it was last applied: the ids of its fields, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedIndex {
    pub fields: Vec<String>,
    pub unique: bool,
}

/// A model as it was last applied in one organization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedModel {
    pub model_id: String,
    pub plugin: String,
    pub name: String,
    /// Field id to what is applied.
    pub fields: BTreeMap<String, AppliedField>,
    /// Index id to the multi-field index applied. Older snapshots have none.
    #[serde(default)]
    pub indexes: BTreeMap<String, AppliedIndex>,
    /// The graph edge tables defined for the model's hierarchy and many2many fields.
    #[serde(default)]
    pub edges: std::collections::BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    CreateTable { table: String, comment: String },
    DefineField { table: String, field: String, surql_type: String, comment: String },
    DefineIndex { table: String, field: String, unique: bool },
    RemoveIndex { table: String, field: String },
    /// A graph edge table: from this model's records to `to` (any table when none).
    DefineEdge { edge: String, from: String, to: Option<String> },
    /// Rebuild a hierarchy's edges from its parent column (the link was just made a hierarchy).
    RebuildEdges { table: String, edge: String, column: String },
    /// An index over several fields (given by id, in order), named after its own id.
    DefineCompositeIndex { table: String, index: String, fields: Vec<String>, unique: bool },
    RemoveCompositeIndex { table: String, index: String },
    /// Give records that lack a value the field's default.
    Backfill { table: String, field: String, default: Value },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub model: String,
    pub table: String,
    pub ops: Vec<Op>,
    /// What stops the upgrade; empty when it can go ahead.
    pub blockers: Vec<String>,
    /// Things worth knowing that need no action (a rename, a field being hidden).
    pub notes: Vec<String>,
    /// What the organization has once this is applied.
    pub snapshot: AppliedModel,
}

impl Plan {
    /// Nothing to do to the database, and nothing new to record.
    pub fn is_noop(&self, applied: Option<&AppliedModel>) -> bool {
        self.ops.is_empty() && applied == Some(&self.snapshot)
    }
}

fn base_type(kind: FieldType) -> &'static str {
    match kind {
        FieldType::String | FieldType::Text | FieldType::Select | FieldType::Link
        | FieldType::Date | FieldType::Datetime => "string",
        FieldType::Int | FieldType::Decimal => "int",
        FieldType::Float => "number",
        FieldType::Bool => "bool",
        FieldType::Json | FieldType::Many2many | FieldType::Child => "any",
    }
}

fn surql_type(kind: FieldType, required: bool) -> String {
    let base = base_type(kind);
    if required || base == "any" { base.to_string() } else { format!("option<{base}>") }
}

/// Changes that lose nothing: the stored values are valid as the new type.
fn widens(from: FieldType, to: FieldType) -> bool {
    use FieldType::*;
    from == to
        || matches!(
            (from, to),
            (Int, Float) | (String, Text) | (Select, String | Text) | (Date | Datetime | Link, String | Text)
        )
}

/// A default as the database keeps it (a decimal is a whole number of its smallest unit).
fn stored_default(field: &FieldDef, default: &Value) -> Value {
    match field.kind {
        FieldType::Decimal => super::decimal::to_scaled(default, field.scale.unwrap_or(super::decimal::DEFAULT_SCALE))
            .map(Value::from)
            .unwrap_or_else(|_| default.clone()),
        _ => default.clone(),
    }
}

fn field_snapshot(field: &FieldDef) -> AppliedField {
    AppliedField {
        name: field.name.clone(),
        kind: field.kind,
        required: field.required && !field.deprecated,
        index: field.index,
        deprecated: field.deprecated,
        scale: (field.kind == FieldType::Decimal).then(|| field.scale.unwrap_or(super::decimal::DEFAULT_SCALE)),
        hierarchy: field.hierarchy,
    }
}

fn define_field(table: &str, model: &str, id: &str, name: &str, kind: FieldType, required: bool) -> Op {
    Op::DefineField {
        table: table.to_string(),
        field: id.to_string(),
        surql_type: surql_type(kind, required),
        comment: format!("{model}.{name}"),
    }
}

/// What has to change to take an organization from `applied` (none: the model was never
/// applied there) to `def`. `row_count` is how many records the table holds.
pub fn plan(plugin: &str, def: &ModelDef, applied: Option<&AppliedModel>, row_count: u64) -> Plan {
    let table = def.model_id.clone().unwrap_or_default();
    let model = def.name.clone();
    let mut ops = Vec::new();
    let mut blockers = Vec::new();
    let mut notes = Vec::new();

    if applied.is_none() {
        ops.push(Op::CreateTable { table: table.clone(), comment: format!("{plugin}.{model}") });
    }
    let rows = if applied.is_some() { row_count } else { 0 };
    let mut snapshot = AppliedModel {
        model_id: table.clone(),
        plugin: plugin.to_string(),
        name: model.clone(),
        fields: BTreeMap::new(),
        indexes: BTreeMap::new(),
        edges: std::collections::BTreeSet::new(),
    };

    for field in &def.fields {
        let Some(id) = field.id.as_deref() else { continue };
        let now = field_snapshot(field);
        let before = applied.and_then(|a| a.fields.get(id));
        let name = &field.name;

        let mut redefine = false;
        // Filled in after the column is defined: a schemafull table refuses an unknown column.
        let mut backfill: Option<Op> = None;
        match before {
            None => {
                redefine = true;
                if now.required && rows > 0 {
                    match &field.default {
                        Some(default) => backfill = Some(Op::Backfill { table: table.clone(), field: id.to_string(), default: stored_default(field, default) }),
                        None => blockers.push(format!(
                            "`{model}.{name}` is a new required field but {rows} record(s) exist: give it a `default`, or make it optional"
                        )),
                    }
                }
            }
            Some(before) => {
                if before.name != *name {
                    // Only the column's readable comment is refreshed; no value moves.
                    redefine = true;
                    notes.push(format!("`{model}`: field `{}` is now called `{name}` (no data changes)", before.name));
                }
                if before.kind != now.kind {
                    if widens(before.kind, now.kind) || rows == 0 {
                        redefine = true;
                    } else {
                        blockers.push(format!(
                            "`{model}.{name}` changes type from {} to {} and {rows} record(s) exist: that needs a migration",
                            before.kind.as_str(),
                            now.kind.as_str()
                        ));
                    }
                }
                if before.kind == now.kind && before.scale != now.scale {
                    if rows > 0 {
                        blockers.push(format!(
                            "`{model}.{name}` changes its decimal scale and {rows} record(s) exist: that needs a migration"
                        ));
                    } else {
                        redefine = true;
                    }
                }
                if before.required != now.required {
                    redefine = true;
                    if now.required && rows > 0 {
                        match &field.default {
                            Some(default) => backfill = Some(Op::Backfill { table: table.clone(), field: id.to_string(), default: stored_default(field, default) }),
                            None => blockers.push(format!(
                                "`{model}.{name}` becomes required but {rows} record(s) exist: give it a `default`"
                            )),
                        }
                    }
                }
                if before.deprecated != now.deprecated {
                    redefine = true;
                    if now.deprecated {
                        notes.push(format!("`{model}.{name}` is hidden; its data is kept"));
                    }
                }
            }
        }
        if redefine && field.kind.has_column() {
            ops.push(define_field(&table, &model, id, name, field.kind, now.required));
        }
        ops.extend(backfill);

        // An index follows the field, but a hidden field has none.
        let wanted = if now.deprecated { None } else { now.index };
        let had = before.and_then(|b| if b.deprecated { None } else { b.index });
        if wanted != had {
            if had.is_some() {
                ops.push(Op::RemoveIndex { table: table.clone(), field: id.to_string() });
            }
            if let Some(kind) = wanted {
                ops.push(Op::DefineIndex { table: table.clone(), field: id.to_string(), unique: kind == IndexKind::Unique });
            }
        }
        snapshot.fields.insert(id.to_string(), now);
    }

    // A field that is gone from the definition is hidden, not deleted.
    if let Some(applied) = applied {
        for (id, before) in &applied.fields {
            if snapshot.fields.contains_key(id) {
                continue;
            }
            let mut hidden = before.clone();
            hidden.deprecated = true;
            hidden.required = false;
            hidden.index = None;
            if !before.deprecated {
                notes.push(format!("`{model}.{}` was removed: it is hidden and its data is kept", before.name));
                if before.kind.has_column() {
                    ops.push(define_field(&table, &model, id, &before.name, before.kind, false));
                }
                if before.index.is_some() {
                    ops.push(Op::RemoveIndex { table: table.clone(), field: id.clone() });
                }
            }
            snapshot.fields.insert(id.clone(), hidden);
        }
    }

    plan_edges(def, applied, &table, &mut ops, &mut snapshot);
    plan_indexes(def, applied, &snapshot.fields.clone(), &table, &mut ops, &mut snapshot);
    Plan { model, table, ops, blockers, notes, snapshot }
}

/// Graph edge tables for hierarchy links and many2many fields. A table is defined once and kept
/// when its field goes away (the data stays, like a hidden column). A link that becomes a
/// hierarchy over existing records has its edges built from the parent column.
fn plan_edges(def: &ModelDef, applied: Option<&AppliedModel>, table: &str, ops: &mut Vec<Op>, snapshot: &mut AppliedModel) {
    if let Some(applied) = applied {
        snapshot.edges = applied.edges.clone();
    }
    for field in def.live_fields() {
        let Some(id) = field.id.as_deref() else { continue };
        let edge = super::runtime::edge_table(table, id);
        let to = match (field.kind, field.hierarchy) {
            (FieldType::Many2many, _) => None,
            (_, true) => Some(table.to_string()),
            _ => continue,
        };
        let before = applied.and_then(|a| a.fields.get(id));
        let had_edges = snapshot.edges.contains(&edge);
        if !had_edges {
            ops.push(Op::DefineEdge { edge: edge.clone(), from: table.to_string(), to });
            snapshot.edges.insert(edge.clone());
        }
        // Turned on for a link that already holds parents: build the edges from the column.
        if field.hierarchy && before.is_some_and(|b| !b.hierarchy) {
            ops.push(Op::RebuildEdges { table: table.to_string(), edge, column: id.to_string() });
        }
    }
}

/// Multi-field indexes follow the definition: new ones are defined, changed ones rebuilt, ones
/// that are gone dropped. Fields are held by id, so renaming a field never touches an index.
fn plan_indexes(
    def: &ModelDef,
    applied: Option<&AppliedModel>,
    fields: &BTreeMap<String, AppliedField>,
    table: &str,
    ops: &mut Vec<Op>,
    snapshot: &mut AppliedModel,
) {
    let id_of = |name: &str| def.fields.iter().find(|f| f.name == name).and_then(|f| f.id.clone());
    for index in &def.indexes {
        let Some(index_id) = index.id.clone() else { continue };
        let ids: Vec<String> = index.fields.iter().filter_map(|name| id_of(name)).collect();
        // A field that is hidden takes its indexes with it, and one that is missing is a
        // definition error reported elsewhere.
        if ids.len() != index.fields.len() || ids.iter().any(|id| fields.get(id).is_none_or(|f| f.deprecated)) {
            continue;
        }
        let now = AppliedIndex { fields: ids.clone(), unique: index.unique };
        let before = applied.and_then(|a| a.indexes.get(&index_id));
        if before != Some(&now) {
            if before.is_some() {
                ops.push(Op::RemoveCompositeIndex { table: table.to_string(), index: index_id.clone() });
            }
            ops.push(Op::DefineCompositeIndex { table: table.to_string(), index: index_id.clone(), fields: ids, unique: index.unique });
        }
        snapshot.indexes.insert(index_id, now);
    }
    if let Some(applied) = applied {
        for index_id in applied.indexes.keys() {
            if !snapshot.indexes.contains_key(index_id) {
                ops.push(Op::RemoveCompositeIndex { table: table.to_string(), index: index_id.clone() });
            }
        }
    }
}

fn statements(plan: &Plan) -> Vec<String> {
    let mut out = Vec::new();
    let mut binds = 0;
    for op in &plan.ops {
        out.push(match op {
            Op::CreateTable { table, comment } => {
                format!("DEFINE TABLE IF NOT EXISTS {table} SCHEMAFULL COMMENT '{comment}'")
            }
            Op::DefineField { table, field, surql_type, comment } => {
                format!("DEFINE FIELD OVERWRITE {field} ON TABLE {table} TYPE {surql_type} COMMENT '{comment}'")
            }
            Op::DefineIndex { table, field, unique } => format!(
                "DEFINE INDEX OVERWRITE ix_{field} ON TABLE {table} FIELDS {field}{}",
                if *unique { " UNIQUE" } else { "" }
            ),
            Op::RemoveIndex { table, field } => format!("REMOVE INDEX IF EXISTS ix_{field} ON TABLE {table}"),
            Op::DefineEdge { edge, from, to } => format!(
                "DEFINE TABLE IF NOT EXISTS {edge} TYPE RELATION IN {from}{} ENFORCED",
                to.as_ref().map(|to| format!(" OUT {to}")).unwrap_or_default()
            ),
            Op::RebuildEdges { table, edge, column } => format!(
                "DELETE {edge}; FOR $r IN (SELECT id, {column} AS p FROM {table} WHERE {column} != NONE) {{ RELATE ($r.p)->{edge}->($r.id); }}"
            ),
            Op::DefineCompositeIndex { table, index, fields, unique } => format!(
                "DEFINE INDEX OVERWRITE ix_{index} ON TABLE {table} FIELDS {}{}",
                fields.join(", "),
                if *unique { " UNIQUE" } else { "" }
            ),
            Op::RemoveCompositeIndex { table, index } => format!("REMOVE INDEX IF EXISTS ix_{index} ON TABLE {table}"),
            Op::Backfill { table, field, .. } => {
                binds += 1;
                format!("UPDATE {table} SET {field} = $backfill_{binds} WHERE {field} = NONE")
            }
        });
    }
    out
}

/// Work out the plan for every model of `plugin` in the organization `db` is on.
pub async fn plan_models(
    db: &Surreal<Client>,
    plugin: &str,
    models: &[ModelDef],
) -> Result<Vec<(Plan, Option<AppliedModel>)>, ApplyError> {
    let mut plans = Vec::with_capacity(models.len());
    for def in models {
        let table = def.model_id.clone().unwrap_or_default();
        let applied = read_applied(db, &table, &def.name).await?;
        let rows = if applied.is_some() { count_rows(db, &table).await? } else { 0 };
        plans.push((plan(plugin, def, applied.as_ref(), rows), applied));
    }
    Ok(plans)
}

/// The problems that stop any of `plans`, each prefixed with its model.
pub fn blockers(plans: &[(Plan, Option<AppliedModel>)]) -> Vec<(String, Vec<String>)> {
    plans
        .iter()
        .filter(|(plan, _)| !plan.blockers.is_empty())
        .map(|(plan, _)| (plan.model.clone(), plan.blockers.clone()))
        .collect()
}

/// Apply the plans (all of them are checked first: one blocked model blocks the lot). Each
/// model's change is one transaction.
pub async fn apply_plans(db: &Surreal<Client>, plans: &[(Plan, Option<AppliedModel>)]) -> Result<(), ApplyError> {
    if let Some((model, problems)) = blockers(plans).into_iter().next() {
        return Err(ApplyError::Blocked { model, problems });
    }
    for (plan, applied) in plans {
        if plan.is_noop(applied.as_ref()) {
            continue;
        }
        let mut script = String::from("BEGIN TRANSACTION;\n");
        for statement in statements(plan) {
            script.push_str(&statement);
            script.push_str(";\n");
        }
        script.push_str(
            "UPSERT type::record('model_schema', $table) CONTENT { model_id: $table, plugin: $plugin, name: $name, snapshot: $snapshot, applied_at: time::now() };\nCOMMIT TRANSACTION;",
        );
        let mut query = db
            .query(script)
            .bind(("table", plan.table.clone()))
            .bind(("plugin", plan.snapshot.plugin.clone()))
            .bind(("name", plan.model.clone()))
            .bind(("snapshot", serde_json::to_value(&plan.snapshot).map_err(|source| ApplyError::Snapshot { model: plan.model.clone(), source })?));
        let mut index = 0;
        for op in &plan.ops {
            if let Op::Backfill { default, .. } = op {
                index += 1;
                query = query.bind((format!("backfill_{index}"), default.clone()));
            }
        }
        query.await?.check()?;
    }
    Ok(())
}

async fn read_applied(db: &Surreal<Client>, table: &str, model: &str) -> Result<Option<AppliedModel>, ApplyError> {
    let mut response = db
        .query("SELECT VALUE snapshot FROM model_schema WHERE model_id = $table;")
        .bind(("table", table.to_string()))
        .await?
        .check()?;
    let snapshots: Vec<Value> = response.take(0)?;
    snapshots
        .into_iter()
        .next()
        .map(|snapshot| {
            serde_json::from_value(snapshot).map_err(|source| ApplyError::Snapshot { model: model.to_string(), source })
        })
        .transpose()
}

async fn count_rows(db: &Surreal<Client>, table: &str) -> Result<u64, ApplyError> {
    let mut response = db
        .query("SELECT count() AS count FROM type::table($table) GROUP ALL;")
        .bind(("table", table.to_string()))
        .await?
        .check()?;
    let rows: Vec<Value> = response.take(0)?;
    Ok(rows.first().and_then(|row| row.get("count")).and_then(Value::as_u64).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_model::definition::sync_ids;
    use serde_json::json;

    fn model(fields: Value) -> ModelDef {
        let mut model: ModelDef = serde_json::from_value(json!({ "name": "note", "fields": fields })).unwrap_or_else(|e| panic!("{e}"));
        sync_ids(&mut model);
        model
    }

    fn ops_of(plan: &Plan) -> Vec<String> {
        statements(plan)
    }

    fn first() -> ModelDef {
        model(json!([
            { "name": "title", "type": "string", "required": true },
            { "name": "pages", "type": "int" },
            { "name": "code", "type": "string", "index": "unique" }
        ]))
    }

    #[test]
    fn a_new_model_gets_its_table_fields_and_indexes() {
        let def = first();
        let plan = plan("notes", &def, None, 0);
        let sql = ops_of(&plan).join("\n");
        assert!(plan.blockers.is_empty());
        assert!(sql.contains(&format!("DEFINE TABLE IF NOT EXISTS {} SCHEMAFULL COMMENT 'notes.note'", def.model_id.clone().unwrap())));
        assert!(sql.contains("TYPE string COMMENT 'note.title'"), "required: not optional\n{sql}");
        assert!(sql.contains("TYPE option<int> COMMENT 'note.pages'"), "{sql}");
        assert!(sql.contains("UNIQUE"), "{sql}");
        assert_eq!(plan.snapshot.fields.len(), 3);
    }

    #[test]
    fn renaming_relabelling_and_reordering_change_nothing() {
        let before = first();
        let applied = plan("notes", &before, None, 0).snapshot;
        let mut after = before.clone();
        after.fields[0].name = "heading".into();
        after.fields[0].label = Some("Heading".into());
        after.fields.reverse();
        after.label = Some("Renamed model".into());
        let plan = plan("notes", &after, Some(&applied), 10_000);
        // Only the renamed column's comment is refreshed: no table, no backfill, no index, no data.
        assert!(plan.ops.iter().all(|op| matches!(op, Op::DefineField { .. })), "{:?}", plan.ops);
        assert_eq!(plan.ops.len(), 1);
        assert!(plan.blockers.is_empty());
        assert!(plan.notes.iter().any(|n| n.contains("heading")), "the rename is noted: {:?}", plan.notes);
    }

    #[test]
    fn adding_an_optional_field_defines_it_and_touches_no_records() {
        let before = first();
        let applied = plan("notes", &before, None, 0).snapshot;
        let mut after = before.clone();
        after.fields.push(serde_json::from_value(json!({ "name": "tag", "type": "string" })).unwrap());
        sync_ids(&mut after);
        let plan = plan("notes", &after, Some(&applied), 5_000);
        assert_eq!(plan.ops.len(), 1);
        assert!(matches!(&plan.ops[0], Op::DefineField { surql_type, .. } if surql_type == "option<string>"));
    }

    #[test]
    fn a_new_required_field_over_existing_records_needs_a_default() {
        let before = first();
        let applied = plan("notes", &before, None, 0).snapshot;
        let mut after = before.clone();
        after.fields.push(serde_json::from_value(json!({ "name": "owner", "type": "string", "required": true })).unwrap());
        sync_ids(&mut after);
        let blocked = plan("notes", &after, Some(&applied), 3);
        assert!(blocked.blockers.iter().any(|b| b.contains("`note.owner`") && b.contains("default")), "{:?}", blocked.blockers);

        // With no records it is fine; with a default the records are filled in.
        assert!(plan("notes", &after, Some(&applied), 0).blockers.is_empty());
        after.fields[3].default = Some(json!("nobody"));
        let filled = plan("notes", &after, Some(&applied), 3);
        assert!(filled.blockers.is_empty());
        assert!(filled.ops.iter().any(|op| matches!(op, Op::Backfill { default, .. } if default == "nobody")));
        let define = filled.ops.iter().position(|op| matches!(op, Op::DefineField { .. }));
        let fill = filled.ops.iter().position(|op| matches!(op, Op::Backfill { .. }));
        assert!(define < fill, "the column must exist before it is filled: {:?}", filled.ops);
    }

    #[test]
    fn removing_a_field_hides_it_and_keeps_its_column_and_id() {
        let before = first();
        let applied = plan("notes", &before, None, 0).snapshot;
        let pages_id = before.fields[1].id.clone().unwrap();
        let mut after = before.clone();
        after.fields.remove(1);
        let plan = plan("notes", &after, Some(&applied), 100);
        assert!(plan.blockers.is_empty());
        assert!(plan.ops.iter().all(|op| !matches!(op, Op::Backfill { .. })));
        assert!(plan.snapshot.fields[&pages_id].deprecated, "still known, so the id is never reused");
        assert!(plan.notes.iter().any(|n| n.contains("hidden")));
        assert!(ops_of(&plan).iter().all(|s| !s.contains("REMOVE FIELD")), "the column is never dropped");

        // Bringing it back (same id) restores it.
        let mut again = before.clone();
        again.fields[1].id = Some(pages_id.clone());
        let restored = super::plan("notes", &again, Some(&plan.snapshot), 100);
        assert!(!restored.snapshot.fields[&pages_id].deprecated);
    }

    #[test]
    fn type_changes_are_free_when_they_lose_nothing_and_blocked_otherwise() {
        let before = first();
        let applied = plan("notes", &before, None, 0).snapshot;
        let mut widened = before.clone();
        widened.fields[1].kind = FieldType::Float;
        assert!(plan("notes", &widened, Some(&applied), 100).blockers.is_empty(), "int to float loses nothing");
        let mut narrowed = before.clone();
        narrowed.fields[1].kind = FieldType::String;
        let blocked = plan("notes", &narrowed, Some(&applied), 100);
        assert!(blocked.blockers.iter().any(|b| b.contains("int to string") && b.contains("migration")), "{:?}", blocked.blockers);
        assert!(plan("notes", &narrowed, Some(&applied), 0).blockers.is_empty(), "an empty table can change type");
    }

    #[test]
    fn indexes_follow_their_fields() {
        let before = first();
        let applied = plan("notes", &before, None, 0).snapshot;
        let mut after = before.clone();
        after.fields[0].index = Some(IndexKind::Plain);
        after.fields[2].index = None;
        let ops = plan("notes", &after, Some(&applied), 1).ops;
        assert!(ops.iter().any(|op| matches!(op, Op::DefineIndex { unique: false, .. })));
        assert!(ops.iter().any(|op| matches!(op, Op::RemoveIndex { .. })));
    }

    #[test]
    fn multi_field_indexes_are_planned_rebuilt_and_dropped() {
        let mut def = first();
        def.indexes.push(super::super::definition::IndexDef { id: Some("idx_abcdef".into()), fields: vec!["title".into(), "pages".into()], unique: false });
        let first_plan = plan("notes", &def, None, 0);
        assert!(first_plan.ops.iter().any(|op| matches!(op, Op::DefineCompositeIndex { fields, unique: false, .. } if fields.len() == 2)));
        let applied = first_plan.snapshot;
        assert!(plan("notes", &def, Some(&applied), 5).is_noop(Some(&applied)));

        // Renaming a field leaves the index alone: it is held by field id.
        let mut renamed = def.clone();
        renamed.fields[0].name = "heading".into();
        renamed.indexes[0].fields[0] = "heading".into();
        assert!(!plan("notes", &renamed, Some(&applied), 5).ops.iter().any(|op| matches!(op, Op::DefineCompositeIndex { .. } | Op::RemoveCompositeIndex { .. })));

        // Making it unique rebuilds it; removing it drops it.
        let mut unique = def.clone();
        unique.indexes[0].unique = true;
        let ops = plan("notes", &unique, Some(&applied), 5).ops;
        assert!(ops.iter().any(|op| matches!(op, Op::RemoveCompositeIndex { .. })));
        assert!(ops.iter().any(|op| matches!(op, Op::DefineCompositeIndex { unique: true, .. })));
        let mut gone = def;
        gone.indexes.clear();
        assert!(plan("notes", &gone, Some(&applied), 5).ops.iter().any(|op| matches!(op, Op::RemoveCompositeIndex { .. })));
    }

    #[test]
    fn an_old_snapshot_without_indexes_still_reads() -> Result<(), serde_json::Error> {
        let old = json!({ "model_id": "mdl_x", "plugin": "p", "name": "n", "fields": {} });
        let applied: AppliedModel = serde_json::from_value(old)?;
        assert!(applied.indexes.is_empty());
        Ok(())
    }

    #[test]
    fn a_decimal_scale_is_fixed_once_records_exist() {
        let mut def = model(json!([{ "name": "pay", "type": "decimal" }]));
        let applied = plan("hr", &def, None, 0).snapshot;
        assert_eq!(applied.fields.values().next().and_then(|f| f.scale), Some(2));
        def.fields[0].scale = Some(3);
        assert!(!plan("hr", &def, Some(&applied), 4).blockers.is_empty());
        assert!(plan("hr", &def, Some(&applied), 0).blockers.is_empty());
    }

    #[test]
    fn an_unchanged_model_is_a_noop() {
        let def = first();
        let applied = plan("notes", &def, None, 0).snapshot;
        let again = plan("notes", &def, Some(&applied), 50);
        assert!(again.is_noop(Some(&applied)));
    }
}
