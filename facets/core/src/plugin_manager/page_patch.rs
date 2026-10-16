//! Changing another plugin's page without copying it.
//!
//! A page that `extends` another and holds `<xpath>` elements is a *patch*. Each element
//! finds one node of the page it extends and does something to it:
//!
//! ```xml
//! <page extends="/hr/employees" model="employee_badge" title="Badge column">
//!   <xpath expr="//column[@field='status']" position="after">
//!     <column field="badge_no" label="Badge"/>
//!   </xpath>
//!   <xpath expr="//field[@name='work_phone']" position="attributes" set_label="Desk phone"/>
//!   <xpath expr="//filter[@field='status']" position="remove"/>
//! </page>
//! ```
//!
//! | `position` | what happens to the node `expr` finds |
//! |---|---|
//! | `after`, `before` | the children are added next to it |
//! | `inside` | the children are added at the end of it |
//! | `replace` | it is replaced by the children |
//! | `remove` | it is removed |
//! | `attributes` | each `set_<name>="value"` sets that attribute; an empty value removes it |
//!
//! `expr` is a small subset of XPath: steps joined by `/` (a child) or `//` (any
//! descendant); each step is a tag name or `*`, with `[@name='value']` conditions and an
//! optional `[n]` (the n-th match, from 1). Only the first match is changed. A patch that matches nothing is skipped and reported; it never
//! breaks the page it patches.
//!
//! Tags are the page's own (`view`, `column`, `field`, …). `@type` on a `view` is its view
//! type (`list`, `form`), which the page parser stores as `viewType`.

use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Axis {
    Child,
    Descendant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Step {
    axis: Axis,
    /// `None` is `*`.
    tag: Option<String>,
    conditions: Vec<(String, String)>,
    /// 1-based position among the matches of this step.
    nth: Option<usize>,
}

/// Parse `expr`; a message says what is wrong with it.
fn parse(expr: &str) -> Result<Vec<Step>, String> {
    let text = expr.trim();
    if !text.starts_with('/') {
        return Err("must start with `/` or `//`".into());
    }
    let mut steps = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let axis = if let Some(after) = rest.strip_prefix("//") {
            rest = after;
            Axis::Descendant
        } else if let Some(after) = rest.strip_prefix('/') {
            rest = after;
            Axis::Child
        } else {
            return Err("steps are separated by `/` or `//`".into());
        };
        // A step runs to the next `/` that is not inside a `[…]`.
        let mut depth = 0usize;
        let mut end = rest.len();
        for (index, ch) in rest.char_indices() {
            match ch {
                '[' => depth += 1,
                ']' => depth = depth.saturating_sub(1),
                '/' if depth == 0 => {
                    end = index;
                    break;
                }
                _ => {}
            }
        }
        let (step_text, tail) = rest.split_at(end);
        rest = tail;
        steps.push(parse_step(axis, step_text)?);
    }
    if steps.is_empty() {
        return Err("names no element".into());
    }
    Ok(steps)
}

fn parse_step(axis: Axis, text: &str) -> Result<Step, String> {
    let (tag, mut conditions_text) = match text.find('[') {
        Some(index) => text.split_at(index),
        None => (text, ""),
    };
    let tag = tag.trim();
    let valid_tag = tag == "*"
        || (!tag.is_empty()
            && tag
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));
    if !valid_tag {
        return Err(format!("`{tag}` is not an element name"));
    }
    let mut conditions = Vec::new();
    let mut nth = None;
    while !conditions_text.is_empty() {
        let inner = conditions_text
            .strip_prefix('[')
            .and_then(|s| s.split_once(']'))
            .ok_or("a condition is written `[@name='value']`")?;
        let (condition, after) = inner;
        conditions_text = after;
        let condition = condition.trim();
        if let Ok(position) = condition.parse::<usize>() {
            if position == 0 {
                return Err("positions count from 1".into());
            }
            nth = Some(position);
        } else if let Some(pair) = condition.strip_prefix('@') {
            let (name, value) = pair
                .split_once('=')
                .ok_or("a condition is written `[@name='value']`")?;
            let value = value.trim();
            let value = value
                .strip_prefix('\'')
                .and_then(|v| v.strip_suffix('\''))
                .or_else(|| value.strip_prefix('"').and_then(|v| v.strip_suffix('"')))
                .ok_or("a condition's value is quoted")?;
            conditions.push((name.trim().to_string(), value.to_string()));
        } else {
            return Err(format!("unsupported condition `[{condition}]`"));
        }
    }
    Ok(Step {
        axis,
        tag: (tag != "*").then(|| tag.to_string()),
        conditions,
        nth,
    })
}

/// `type` is the element's name in the tree, so an XML `type` attribute is `viewType`.
fn attribute_name(name: &str) -> &str {
    if name == "type" { "viewType" } else { name }
}

fn matches(node: &Value, step: &Step) -> bool {
    let Some(object) = node.as_object() else {
        return false;
    };
    if let Some(tag) = &step.tag
        && object.get("type").and_then(Value::as_str) != Some(tag.as_str())
    {
        return false;
    }
    step.conditions
        .iter()
        .all(|(name, wanted)| match object.get(attribute_name(name)) {
            Some(Value::String(value)) => value == wanted,
            Some(Value::Bool(value)) => value.to_string() == *wanted,
            Some(Value::Number(value)) => value.to_string() == *wanted,
            _ => false,
        })
}

fn children_of(node: &Value) -> &[Value] {
    node.get("children")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// Paths (child indexes from the root) of the nodes `steps` selects, in document order.
fn select(root: &Value, steps: &[Step]) -> Vec<Vec<usize>> {
    // The first step is taken against a virtual parent whose only child is the root, so
    // `//page` and `/page` both reach the root.
    let mut current: Vec<Vec<usize>> = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        let mut found: Vec<Vec<usize>> = Vec::new();
        let origins: Vec<Option<Vec<usize>>> = if index == 0 {
            vec![None]
        } else {
            current.iter().cloned().map(Some).collect()
        };
        for origin in origins {
            let mut candidates: Vec<Vec<usize>> = Vec::new();
            match (&origin, &step.axis) {
                (None, Axis::Child) => {
                    if matches(root, step) {
                        candidates.push(Vec::new());
                    }
                }
                (None, Axis::Descendant) => {
                    if matches(root, step) {
                        candidates.push(Vec::new());
                    }
                    descend(root, &mut Vec::new(), step, &mut candidates);
                }
                (Some(path), Axis::Child) => {
                    if let Some(node) = node_at(root, path) {
                        for (child_index, child) in children_of(node).iter().enumerate() {
                            if matches(child, step) {
                                let mut at = path.clone();
                                at.push(child_index);
                                candidates.push(at);
                            }
                        }
                    }
                }
                (Some(path), Axis::Descendant) => {
                    if let Some(node) = node_at(root, path) {
                        let mut from = path.clone();
                        descend(node, &mut from, step, &mut candidates);
                    }
                }
            }
            found.extend(candidates);
        }
        if let Some(nth) = step.nth {
            found = found.into_iter().nth(nth - 1).into_iter().collect();
        }
        current = found;
        if current.is_empty() {
            break;
        }
    }
    current
}

fn descend(node: &Value, path: &mut Vec<usize>, step: &Step, out: &mut Vec<Vec<usize>>) {
    for (index, child) in children_of(node).iter().enumerate() {
        path.push(index);
        if matches(child, step) {
            out.push(path.clone());
        }
        descend(child, path, step, out);
        path.pop();
    }
}

fn node_at<'a>(root: &'a Value, path: &[usize]) -> Option<&'a Value> {
    let mut node = root;
    for &index in path {
        node = node.get("children")?.as_array()?.get(index)?;
    }
    Some(node)
}

fn node_at_mut<'a>(root: &'a mut Value, path: &[usize]) -> Option<&'a mut Value> {
    let mut node = root;
    for &index in path {
        node = node.get_mut("children")?.as_array_mut()?.get_mut(index)?;
    }
    Some(node)
}

/// Whether `page` (a contributed page's tree) is a patch: it has `<xpath>` children.
pub fn is_patch(page: &Value) -> bool {
    children_of(page)
        .iter()
        .any(|child| child.get("type").and_then(Value::as_str) == Some("xpath"))
}

/// Check a page's `<xpath>` elements without applying them: the expression parses, the
/// position is known, and `inside`/`after`/`before`/`replace` carry something to put there.
/// Pages with no `<xpath>` elements are valid.
pub fn validate(page: &Value) -> Result<(), String> {
    for op in children_of(page) {
        if op.get("type").and_then(Value::as_str) != Some("xpath") {
            continue;
        }
        let expr = op
            .get("expr")
            .and_then(Value::as_str)
            .ok_or("an <xpath> needs an `expr`")?;
        parse(expr).map_err(|reason| format!("<xpath expr=\"{expr}\">: {reason}"))?;
        let position = op
            .get("position")
            .and_then(Value::as_str)
            .unwrap_or("inside");
        match position {
            "after" | "before" | "inside" | "replace" if children_of(op).is_empty() => {
                return Err(format!(
                    "<xpath expr=\"{expr}\" position=\"{position}\"> needs content to put there"
                ));
            }
            "after" | "before" | "inside" | "replace" | "remove" | "attributes" => {}
            other => {
                return Err(format!(
                    "<xpath expr=\"{expr}\">: unknown position `{other}` (after, before, inside, replace, remove, attributes)"
                ));
            }
        }
        if position == "attributes"
            && !op
                .as_object()
                .is_some_and(|o| o.keys().any(|k| k.starts_with("set_")))
        {
            return Err(format!(
                "<xpath expr=\"{expr}\" position=\"attributes\"> needs at least one `set_<name>` attribute"
            ));
        }
    }
    Ok(())
}

/// Apply the `<xpath>` elements of `patch` to `tree`, in order. Returns what could not be
/// applied, for the log; the rest are applied.
pub fn apply(tree: &mut Value, patch: &Value) -> Vec<String> {
    let mut problems = Vec::new();
    for op in children_of(patch).to_vec() {
        if op.get("type").and_then(Value::as_str) != Some("xpath") {
            continue;
        }
        let expr = op.get("expr").and_then(Value::as_str).unwrap_or("");
        let position = op
            .get("position")
            .and_then(Value::as_str)
            .unwrap_or("inside");
        if let Err(problem) = apply_one(tree, &op, expr, position) {
            problems.push(format!("`{expr}` ({position}): {problem}"));
        }
    }
    problems
}

fn apply_one(tree: &mut Value, op: &Value, expr: &str, position: &str) -> Result<(), String> {
    let steps = parse(expr)?;
    let path = select(tree, &steps)
        .into_iter()
        .next()
        .ok_or("matches nothing")?;
    let content: Vec<Value> = children_of(op).to_vec();

    // The page itself can be added to or have its attributes set, nothing else.
    let Some((&index, parent_path)) = path.split_last() else {
        return match position {
            "inside" => {
                push_all(tree, content);
                Ok(())
            }
            "attributes" => {
                set_attributes(tree, op);
                Ok(())
            }
            _ => Err("the page itself can only be added to or have attributes set".into()),
        };
    };

    match position {
        "inside" => {
            let target = node_at_mut(tree, &path).ok_or("matches nothing")?;
            push_all(target, content);
        }
        "attributes" => {
            let target = node_at_mut(tree, &path).ok_or("matches nothing")?;
            set_attributes(target, op);
        }
        "after" | "before" | "replace" | "remove" => {
            let parent = node_at_mut(tree, parent_path).ok_or("matches nothing")?;
            let list = parent
                .get_mut("children")
                .and_then(Value::as_array_mut)
                .ok_or("matches nothing")?;
            match position {
                "after" => {
                    for (offset, item) in content.into_iter().enumerate() {
                        list.insert(index + 1 + offset, item);
                    }
                }
                "before" => {
                    for (offset, item) in content.into_iter().enumerate() {
                        list.insert(index + offset, item);
                    }
                }
                "replace" => {
                    list.remove(index);
                    for (offset, item) in content.into_iter().enumerate() {
                        list.insert(index + offset, item);
                    }
                }
                _ => {
                    list.remove(index);
                }
            }
        }
        other => {
            return Err(format!(
                "unknown position `{other}` (after, before, inside, replace, remove, attributes)"
            ));
        }
    }
    Ok(())
}

fn push_all(node: &mut Value, content: Vec<Value>) {
    if let Some(object) = node.as_object_mut() {
        let children = object
            .entry("children")
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Some(list) = children.as_array_mut() {
            list.extend(content);
        }
    }
}

/// The attributes an `attributes` patch may not touch: they say what the node is.
const PROTECTED: &[&str] = &["type", "children"];

fn set_attributes(node: &mut Value, op: &Value) {
    let (Some(target), Some(source)) = (node.as_object_mut(), op.as_object()) else {
        return;
    };
    let mut changes = Map::new();
    for (key, value) in source {
        if let Some(name) = key.strip_prefix("set_")
            && !name.is_empty()
            && !PROTECTED.contains(&name)
        {
            changes.insert(attribute_name(name).to_string(), value.clone());
        }
    }
    for (name, value) in changes {
        if value.as_str() == Some("") {
            target.remove(&name);
        } else {
            target.insert(name, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn host() -> Value {
        json!({ "type": "page", "model": "employee", "children": [
            { "type": "view", "viewType": "list", "children": [
                { "type": "search", "fields": "display_name" },
                { "type": "columns", "children": [
                    { "type": "column", "field": "employee_no" },
                    { "type": "column", "field": "status", "label": "Status" },
                    { "type": "column", "field": "hire_date" }
                ] }
            ] },
            { "type": "view", "viewType": "form", "children": [
                { "type": "group", "title": "Contact", "children": [
                    { "type": "field", "name": "work_email" },
                    { "type": "field", "name": "work_phone", "required": true }
                ] }
            ] }
        ] })
    }

    fn patch(ops: Value) -> Value {
        json!({ "type": "page", "extends": "/hr/employees", "children": ops })
    }

    fn columns(tree: &Value) -> Vec<String> {
        let path = select(tree, &parse("//columns").unwrap()).remove(0);
        children_of(node_at(tree, &path).unwrap())
            .iter()
            .map(|c| c["field"].as_str().unwrap_or("").to_string())
            .collect()
    }

    #[test]
    fn parses_expressions() {
        let steps = parse("//view[@type='list']/columns/column[2]").unwrap();
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].axis, Axis::Descendant);
        assert_eq!(steps[0].conditions, [("type".into(), "list".into())]);
        assert_eq!(steps[2].nth, Some(2));
        for bad in [
            "",
            "column",
            "//",
            "//col umn",
            "//a[@b]",
            "//a[0]",
            "//a[@b=c]",
            "//a[text()]",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn adds_a_column_after_another() {
        let mut tree = host();
        let problems = apply(
            &mut tree,
            &patch(
                json!([{ "type": "xpath", "expr": "//column[@field='status']", "position": "after",
                "children": [{ "type": "column", "field": "badge_no", "label": "Badge" }] }]),
            ),
        );
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(
            columns(&tree),
            ["employee_no", "status", "badge_no", "hire_date"]
        );
    }

    #[test]
    fn before_inside_replace_and_remove() {
        let mut tree = host();
        let ops = json!([
            { "type": "xpath", "expr": "//column[@field='employee_no']", "position": "before",
              "children": [{ "type": "column", "field": "photo" }] },
            { "type": "xpath", "expr": "//columns", "position": "inside",
              "children": [{ "type": "column", "field": "last" }] },
            { "type": "xpath", "expr": "//column[@field='hire_date']", "position": "replace",
              "children": [{ "type": "column", "field": "joined" }] },
            { "type": "xpath", "expr": "//search", "position": "remove" }
        ]);
        let problems = apply(&mut tree, &patch(ops));
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(
            columns(&tree),
            ["photo", "employee_no", "status", "joined", "last"]
        );
        assert_eq!(
            tree["children"][0]["children"][0]["type"], "columns",
            "search is gone"
        );
    }

    #[test]
    fn sets_and_clears_attributes() {
        let mut tree = host();
        let ops = json!([
            { "type": "xpath", "expr": "//field[@name='work_phone']", "position": "attributes",
              "set_label": "Desk phone", "set_required": "" },
            { "type": "xpath", "expr": "//column[@field='status']", "position": "attributes",
              "set_label": "", "set_type": "evil", "set_children": "x" }
        ]);
        apply(&mut tree, &patch(ops));
        let phone = &tree["children"][1]["children"][0]["children"][1];
        assert_eq!(phone["label"], "Desk phone");
        assert!(phone.get("required").is_none());
        let status = &tree["children"][0]["children"][1]["children"][1];
        assert!(status.get("label").is_none());
        assert_eq!(status["type"], "column", "what a node is cannot be changed");
        assert!(status["children"].is_null());
    }

    #[test]
    fn conditions_use_view_type_and_position() {
        let mut tree = host();
        let ops = json!([
            { "type": "xpath", "expr": "//view[@type='form']//field[2]", "position": "after",
              "children": [{ "type": "field", "name": "extension" }] },
            { "type": "xpath", "expr": "/page/view[@type='list']/columns/column[1]", "position": "remove" }
        ]);
        let problems = apply(&mut tree, &patch(ops));
        assert!(problems.is_empty(), "{problems:?}");
        let group = &tree["children"][1]["children"][0]["children"];
        assert_eq!(group[2]["name"], "extension");
        assert_eq!(columns(&tree), ["status", "hire_date"]);
    }

    #[test]
    fn only_the_first_match_changes() {
        let mut tree = host();
        let ops = json!([{ "type": "xpath", "expr": "//column", "position": "remove" }]);
        apply(&mut tree, &patch(ops));
        assert_eq!(columns(&tree), ["status", "hire_date"]);
    }

    #[test]
    fn what_matches_nothing_is_reported_and_skipped() {
        let mut tree = host();
        let before = tree.clone();
        let ops = json!([
            { "type": "xpath", "expr": "//column[@field='nope']", "position": "remove" },
            { "type": "xpath", "expr": "//column", "position": "teleport" },
            { "type": "xpath", "expr": "//(", "position": "remove" },
            { "type": "xpath", "expr": "/page", "position": "remove" }
        ]);
        let problems = apply(&mut tree, &patch(ops));
        assert_eq!(problems.len(), 4, "{problems:?}");
        assert_eq!(tree, before);
    }

    #[test]
    fn validates_patches_when_a_plugin_loads() {
        let ok = patch(json!([
            { "type": "xpath", "expr": "//column", "position": "remove" },
            { "type": "xpath", "expr": "//field[@name='a']", "position": "attributes", "set_label": "A" },
            { "type": "xpath", "expr": "//columns", "children": [{ "type": "column" }] }
        ]));
        assert_eq!(validate(&ok), Ok(()));
        for bad in [
            json!([{ "type": "xpath", "position": "remove" }]),
            json!([{ "type": "xpath", "expr": "column", "position": "remove" }]),
            json!([{ "type": "xpath", "expr": "//column", "position": "teleport" }]),
            json!([{ "type": "xpath", "expr": "//column", "position": "after" }]),
            json!([{ "type": "xpath", "expr": "//column", "position": "attributes" }]),
        ] {
            assert!(validate(&patch(bad.clone())).is_err(), "{bad}");
        }
        assert_eq!(
            validate(&json!({ "type": "page", "children": [{ "type": "view" }] })),
            Ok(())
        );
    }

    #[test]
    fn recognises_a_patch() {
        assert!(is_patch(&patch(
            json!([{ "type": "xpath", "expr": "//a" }])
        )));
        assert!(!is_patch(
            &json!({ "type": "page", "children": [{ "type": "view" }] })
        ));
    }
}
