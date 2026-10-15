//! Model hints for page fields.
//!
//! A page names the model it shows (`<page model="employee">`, or `model=` on a view) and
//! its fields by name. The model already knows what each field *is*: a decimal with two
//! digits, a yes/no, one of five statuses. This copies that onto the field nodes when a
//! page is served, so the web app can pick a widget without the author repeating the type
//! in `fieldType="…"` (which still wins when given).
//!
//! The hints are added under `model*` keys and never overwrite what the author wrote:
//!
//! | key | from | for |
//! |---|---|---|
//! | `modelType` | `type` | every hinted field |
//! | `modelLabel` | `label` | a field the page gave no label |
//! | `modelRequired` | `required` | `true` only |
//! | `modelHelp` | `help` | |
//! | `modelScale` | `scale` | `decimal` |
//! | `modelOptions` | `options` | `select`: `[{ value, label?, color? }]` (also on `<statusbar field>`, which lists them as its steps) |
//! | `modelTarget` | `target` | `link`, `many2many` |
//! | `modelMultiple` | `multiple` | `file` |
//! | `groupByTarget` | `target` of the `group_by` field | a view grouped by a `link` |

use std::collections::HashMap;

use serde_json::{Map, Value};

/// Node types whose field is named by `name` (a form field) or `field` (a list column, a
/// filter, a grouping).
const FIELD_NODES: &[&str] = &["field", "column", "filter", "groupby", "statusbar"];

/// Model definitions by model name, as stored in the catalog (`plugin_models.definition`).
pub type ModelDefs = HashMap<String, Value>;

/// Add model hints to every field node of `tree` that names a field of its model.
pub fn annotate_fields(tree: &mut Value, models: &ModelDefs) {
    if models.is_empty() {
        return;
    }
    walk(tree, None, models);
}

fn walk(node: &mut Value, inherited: Option<&str>, models: &ModelDefs) {
    let Some(object) = node.as_object_mut() else {
        return;
    };
    // The nearest `model=` above a field says which model it belongs to. A plugin-qualified
    // name (`hr.employee`) is a model of another plugin, which this page's plugin does not
    // own: leave those alone rather than guess.
    let own = object
        .get("model")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .map(str::to_string);
    let current = own.as_deref().or(inherited);

    if let Some(model) = current.and_then(|name| models.get(name))
        && let Some(kind) = object.get("type").and_then(Value::as_str)
        && FIELD_NODES.contains(&kind)
    {
        let key = if kind == "field" { "name" } else { "field" };
        let field_name = object
            .get(key)
            .or_else(|| object.get("name"))
            .and_then(Value::as_str)
            .map(str::to_string);
        if let Some(field) = field_name.and_then(|name| field_of(model, &name)) {
            add_hints(object, field);
        }
    }

    // A view that groups by a link field (a kanban's `group_by`) is told what it points at, so
    // its column headings can show names.
    if let Some(model) = current.and_then(|name| models.get(name))
        && let Some(group) = object
            .get("group_by")
            .and_then(Value::as_str)
            .map(str::to_string)
        && let Some(field) = field_of(model, &group)
        && field.get("type").and_then(Value::as_str) == Some("link")
        && let Some(target) = field.get("target").and_then(Value::as_str)
    {
        object
            .entry("groupByTarget".to_string())
            .or_insert(Value::String(target.to_string()));
    }

    let current = current.map(str::to_string);
    if let Some(Value::Array(children)) = object.get_mut("children") {
        for child in children {
            walk(child, current.as_deref(), models);
        }
    }
}

fn field_of<'a>(model: &'a Value, name: &str) -> Option<&'a Map<String, Value>> {
    model
        .get("fields")?
        .as_array()?
        .iter()
        .filter_map(Value::as_object)
        .find(|field| field.get("name").and_then(Value::as_str) == Some(name))
}

fn add_hints(node: &mut Map<String, Value>, field: &Map<String, Value>) {
    let mut put = |key: &str, value: Value| {
        node.entry(key.to_string()).or_insert(value);
    };

    let Some(kind) = field.get("type").and_then(Value::as_str) else {
        return;
    };
    put("modelType", Value::String(kind.to_string()));
    if let Some(label) = field.get("label").and_then(Value::as_str) {
        put("modelLabel", Value::String(label.to_string()));
    }
    if field.get("required").and_then(Value::as_bool) == Some(true) {
        put("modelRequired", Value::Bool(true));
    }
    if let Some(help) = field.get("help").and_then(Value::as_str) {
        put("modelHelp", Value::String(help.to_string()));
    }
    match kind {
        "decimal" => {
            // The model's default scale is 2.
            let scale = field.get("scale").cloned().unwrap_or(Value::from(2));
            put("modelScale", scale);
        }
        "select" => {
            if let Some(options) = field.get("options").and_then(Value::as_array) {
                let options: Vec<Value> = options
                    .iter()
                    .filter_map(Value::as_object)
                    .filter_map(|option| {
                        let value = option.get("value")?.clone();
                        let mut out = Map::new();
                        out.insert("value".into(), value);
                        for key in ["label", "color"] {
                            if let Some(extra) = option.get(key) {
                                out.insert(key.into(), extra.clone());
                            }
                        }
                        Some(Value::Object(out))
                    })
                    .collect();
                put("modelOptions", Value::Array(options));
            }
        }
        "link" | "many2many" => {
            if let Some(target) = field.get("target").and_then(Value::as_str) {
                put("modelTarget", Value::String(target.to_string()));
            }
        }
        "file" => {
            if field.get("multiple").and_then(Value::as_bool) == Some(true) {
                put("modelMultiple", Value::Bool(true));
            }
        }
        _ => {}
    }
}

/// Index stored model definitions by name.
pub fn index_models(rows: Vec<(String, Value)>) -> ModelDefs {
    rows.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn models() -> ModelDefs {
        index_models(vec![(
            "invoice".into(),
            json!({
                "name": "invoice",
                "fields": [
                    { "name": "total", "type": "decimal", "scale": 3, "label": "Total", "required": true },
                    { "name": "tax", "type": "decimal" },
                    { "name": "paid", "type": "bool", "help": "Settled in full" },
                    { "name": "status", "type": "select", "options": [
                        { "value": "draft", "label": "Draft", "color": "gray" },
                        { "value": "posted" } ] },
                    { "name": "customer", "type": "link", "target": "party.party" },
                    { "name": "scan", "type": "file", "multiple": true }
                ]
            }),
        )])
    }

    fn page(children: Value) -> Value {
        json!({ "type": "page", "model": "invoice", "children": [
            { "type": "view", "viewType": "form", "children": children } ] })
    }

    fn field<'a>(tree: &'a Value, index: usize) -> &'a Value {
        &tree["children"][0]["children"][index]
    }

    #[test]
    fn a_statusbar_gets_the_options_of_its_field() {
        let mut tree = page(json!([{ "type": "statusbar", "field": "status" }]));
        annotate_fields(&mut tree, &models());
        let bar = field(&tree, 0);
        assert_eq!(bar["modelOptions"][0]["value"], "draft");
        assert_eq!(bar["modelOptions"][0]["label"], "Draft");
        assert_eq!(bar["modelOptions"][1]["value"], "posted");
    }

    #[test]
    fn adds_the_model_type_and_what_the_widget_needs() {
        let mut tree = page(json!([
            { "type": "field", "name": "total" },
            { "type": "field", "name": "tax" },
            { "type": "field", "name": "paid" },
            { "type": "field", "name": "status" },
            { "type": "field", "name": "customer" },
            { "type": "field", "name": "scan" },
        ]));
        annotate_fields(&mut tree, &models());

        let total = field(&tree, 0);
        assert_eq!(total["modelType"], "decimal");
        assert_eq!(total["modelScale"], 3);
        assert_eq!(total["modelLabel"], "Total");
        assert_eq!(total["modelRequired"], true);
        assert_eq!(field(&tree, 1)["modelScale"], 2);
        assert_eq!(field(&tree, 2)["modelHelp"], "Settled in full");
        assert!(field(&tree, 2).get("modelRequired").is_none());

        let status = field(&tree, 3);
        assert_eq!(status["modelType"], "select");
        assert_eq!(status["modelOptions"][0]["color"], "gray");
        assert_eq!(status["modelOptions"][1]["value"], "posted");

        assert_eq!(field(&tree, 4)["modelTarget"], "party.party");
        assert_eq!(field(&tree, 5)["modelMultiple"], true);
    }

    #[test]
    fn never_overwrites_what_the_author_wrote() {
        let mut tree = page(json!([
            { "type": "field", "name": "total", "fieldType": "currency", "modelLabel": "Mine" }
        ]));
        annotate_fields(&mut tree, &models());
        let total = field(&tree, 0);
        assert_eq!(total["fieldType"], "currency");
        assert_eq!(total["modelLabel"], "Mine");
        assert_eq!(total["modelType"], "decimal");
    }

    #[test]
    fn columns_and_filters_are_named_by_field() {
        let mut tree = json!({ "type": "page", "model": "invoice", "children": [
            { "type": "columns", "children": [
                { "type": "column", "field": "status" },
                { "type": "column", "field": "nope" } ] },
            { "type": "filter", "field": "paid" } ] });
        annotate_fields(&mut tree, &models());
        assert_eq!(tree["children"][0]["children"][0]["modelType"], "select");
        assert!(
            tree["children"][0]["children"][1]
                .get("modelType")
                .is_none()
        );
        assert_eq!(tree["children"][1]["modelType"], "bool");
    }

    #[test]
    fn a_view_may_name_its_own_model() {
        let mut tree = json!({ "type": "page", "model": "other", "children": [
            { "type": "view", "model": "invoice", "children": [
                { "type": "field", "name": "paid" } ] } ] });
        annotate_fields(&mut tree, &models());
        assert_eq!(tree["children"][0]["children"][0]["modelType"], "bool");
    }

    #[test]
    fn unknown_models_and_fields_are_left_alone() {
        let mut tree = json!({ "type": "page", "model": "ghost", "children": [
            { "type": "field", "name": "total" } ] });
        let before = tree.clone();
        annotate_fields(&mut tree, &models());
        assert_eq!(tree, before);

        let mut free =
            json!({ "type": "page", "children": [ { "type": "field", "name": "total" } ] });
        let before = free.clone();
        annotate_fields(&mut free, &models());
        assert_eq!(free, before);
    }
}
