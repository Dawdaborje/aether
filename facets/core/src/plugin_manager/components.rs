//! Plugin UI components: reusable XML fragments under `components/`.
//!
//! ```xml
//! <component name="note_card">
//!   <group>
//!     <field name="title" label="{{title}}" />
//!     <slot />
//!   </group>
//! </component>
//! ```
//!
//! Pages (and other components) pull them in with `<use component="note_card" …>`
//! or `<use component="other.note_card" …>`. The catalog stores the definitions;
//! [`expand_tree`] turns `use` nodes into widget trees when a page is served.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use surrealdb::types::SurrealValue;
use thiserror::Error;

use crate::plugin_manager::xml_tree::{XmlError, compile_xml};

/// Folder, relative to a plugin package, that holds component XML.
pub const COMPONENTS_DIR: &str = "components";

/// How deep nested `<use>` may go before it is refused.
pub const MAX_EXPAND_DEPTH: usize = 16;

#[derive(Debug, Error)]
pub enum ComponentError {
    #[error("filesystem error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    Xml(#[from] XmlError),

    #[error("{0}: the root element must be <component>")]
    NotAComponent(PathBuf),

    #[error("{0}: <component> needs a `name` attribute")]
    MissingName(PathBuf),

    #[error(
        "{path}: invalid component name `{name}`: use lowercase letters, digits and `_`, starting with a letter"
    )]
    InvalidName { path: PathBuf, name: String },

    #[error("component `{name}` is declared by both {first} and {second}")]
    DuplicateName {
        name: String,
        first: PathBuf,
        second: PathBuf,
    },

    #[error("{0} is a symbolic link; components must be regular files")]
    Symlink(PathBuf),

    #[error("{path}: {message}")]
    Expand { path: String, message: String },
}

/// One parsed component definition.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentDocument {
    /// Local short name (`note_card`).
    pub name: String,
    /// Every model named by any element (`model="..."`).
    pub models: BTreeSet<String>,
    /// Compiled tree (`type` = `component`, with children as the body).
    pub tree: Value,
    /// Path of the XML file relative to the plugin package.
    pub source: PathBuf,
}

/// A component name that is valid as the local part: lowercase letters, digits
/// and `_`, starting with a letter, at most 32 characters.
pub fn is_component_name(value: &str) -> bool {
    let mut characters = value.chars();
    value.len() <= 32
        && characters
            .next()
            .is_some_and(|first| first.is_ascii_lowercase())
        && characters.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Parse one component file. `source` is only used in messages and stored on the result.
pub fn parse_component(xml: &str, source: &Path) -> Result<ComponentDocument, ComponentError> {
    let root = compile_xml(xml, source)?;
    if root.tag != "component" {
        return Err(ComponentError::NotAComponent(source.to_path_buf()));
    }
    let name = root
        .attribute("name")
        .filter(|name| !name.is_empty())
        .ok_or_else(|| ComponentError::MissingName(source.to_path_buf()))?;
    if !is_component_name(name) {
        return Err(ComponentError::InvalidName {
            path: source.to_path_buf(),
            name: name.to_string(),
        });
    }
    let mut models = BTreeSet::new();
    root.collect_models(&mut models);
    Ok(ComponentDocument {
        name: name.to_string(),
        models,
        tree: root.into_json(),
        source: source.to_path_buf(),
    })
}

/// Find and parse every `*.xml` under `<package_dir>/components`, rejecting
/// duplicate names. Returns nothing when the folder does not exist.
pub async fn discover_components(
    package_dir: &Path,
) -> Result<Vec<ComponentDocument>, ComponentError> {
    let components_dir = package_dir.join(COMPONENTS_DIR);
    let mut files = Vec::new();
    let mut pending = vec![components_dir.clone()];
    while let Some(directory) = pending.pop() {
        let mut entries = match tokio::fs::read_dir(&directory).await {
            Ok(entries) => entries,
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound && directory == components_dir =>
            {
                return Ok(Vec::new());
            }
            Err(source) => {
                return Err(ComponentError::Io {
                    path: directory,
                    source,
                });
            }
        };
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|source| ComponentError::Io {
                path: directory.clone(),
                source,
            })?
        {
            let path = entry.path();
            let file_type = entry
                .file_type()
                .await
                .map_err(|source| ComponentError::Io {
                    path: path.clone(),
                    source,
                })?;
            if file_type.is_symlink() {
                return Err(ComponentError::Symlink(path));
            }
            if file_type.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "xml") {
                files.push(path);
            }
        }
    }
    files.sort();

    let mut components: Vec<ComponentDocument> = Vec::with_capacity(files.len());
    for file in files {
        let relative = file
            .strip_prefix(package_dir)
            .unwrap_or(&file)
            .to_path_buf();
        let xml = tokio::fs::read_to_string(&file)
            .await
            .map_err(|source| ComponentError::Io {
                path: file.clone(),
                source,
            })?;
        let component = parse_component(&xml, &relative)?;
        if let Some(existing) = components
            .iter()
            .find(|existing| existing.name == component.name)
        {
            return Err(ComponentError::DuplicateName {
                name: component.name,
                first: existing.source.clone(),
                second: component.source,
            });
        }
        components.push(component);
    }
    Ok(components)
}

/// A reference found in a tree: `plugin` is `None` for a bare local name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ComponentRef {
    pub plugin: Option<String>,
    pub name: String,
}

impl ComponentRef {
    /// Parse `note_card` or `notes.note_card`.
    pub fn parse(raw: &str) -> Result<Self, String> {
        if raw.is_empty() {
            return Err("component name is empty".into());
        }
        if let Some((plugin, name)) = raw.split_once('.') {
            if plugin.is_empty() || name.is_empty() || name.contains('.') {
                return Err(format!("invalid component reference `{raw}`"));
            }
            if !is_component_name(plugin) {
                return Err(format!("invalid plugin name in `{raw}`"));
            }
            if !is_component_name(name) {
                return Err(format!("invalid component name in `{raw}`"));
            }
            Ok(Self {
                plugin: Some(plugin.to_string()),
                name: name.to_string(),
            })
        } else {
            if !is_component_name(raw) {
                return Err(format!("invalid component name `{raw}`"));
            }
            Ok(Self {
                plugin: None,
                name: raw.to_string(),
            })
        }
    }

    pub fn full_name(&self, default_plugin: &str) -> String {
        match &self.plugin {
            Some(plugin) => format!("{plugin}.{}", self.name),
            None => format!("{default_plugin}.{}", self.name),
        }
    }
}

/// Walk a compiled tree and collect every `<use component="…">` reference.
pub fn collect_uses(tree: &Value) -> Vec<(ComponentRef, String)> {
    let mut found = Vec::new();
    collect_uses_inner(tree, &mut found);
    found
}

fn collect_uses_inner(tree: &Value, found: &mut Vec<(ComponentRef, String)>) {
    let Some(object) = tree.as_object() else {
        return;
    };
    if object.get("type").and_then(Value::as_str) == Some("use")
        && let Some(component) = object.get("component").and_then(Value::as_str)
    {
        match ComponentRef::parse(component) {
            Ok(reference) => found.push((reference, component.to_string())),
            Err(_) => found.push((
                ComponentRef {
                    plugin: None,
                    name: String::new(),
                },
                component.to_string(),
            )),
        }
    }
    if let Some(Value::Array(children)) = object.get("children") {
        for child in children {
            collect_uses_inner(child, found);
        }
    }
}

/// Registry keyed by full name (`plugin.component`) → component root tree.
pub type ComponentRegistry = HashMap<String, Value>;

/// Expand every `<use>` in `tree` against `registry`. Bare names resolve under
/// `owner_plugin`. On success the tree contains only widget tags.
pub fn expand_tree(
    tree: &Value,
    owner_plugin: &str,
    registry: &ComponentRegistry,
) -> Result<Value, ComponentError> {
    let mut stack = Vec::new();
    expand_node(tree, owner_plugin, registry, &mut stack, 0)
}

/// Rewrite a page tree so its body is the default slot of `wrap` (a component
/// ref). The page `title` is passed as the `title` prop. Strips `wrap` from the
/// page node. Call [`expand_tree`] afterwards to resolve slots.
pub fn wrap_page_tree(tree: &Value, wrap: &str, title: &str) -> Result<Value, ComponentError> {
    let Some(object) = tree.as_object() else {
        return Err(ComponentError::Expand {
            path: wrap.into(),
            message: "page tree must be an object".into(),
        });
    };
    if object.get("type").and_then(Value::as_str) != Some("page") {
        return Err(ComponentError::Expand {
            path: wrap.into(),
            message: "wrap applies only to a page root".into(),
        });
    }
    ComponentRef::parse(wrap).map_err(|message| ComponentError::Expand {
        path: wrap.into(),
        message,
    })?;

    let children = object
        .get("children")
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));

    let mut use_node = Map::new();
    use_node.insert("type".into(), Value::String("use".into()));
    use_node.insert("component".into(), Value::String(wrap.into()));
    use_node.insert("title".into(), Value::String(title.into()));
    use_node.insert("children".into(), children);

    let mut out = object.clone();
    out.remove("wrap");
    out.insert(
        "children".into(),
        Value::Array(vec![Value::Object(use_node)]),
    );
    Ok(Value::Object(out))
}

fn expand_node(
    tree: &Value,
    owner_plugin: &str,
    registry: &ComponentRegistry,
    stack: &mut Vec<String>,
    depth: usize,
) -> Result<Value, ComponentError> {
    let Some(object) = tree.as_object() else {
        return Ok(tree.clone());
    };
    if object.get("type").and_then(Value::as_str) == Some("use") {
        return expand_use(object, owner_plugin, registry, stack, depth);
    }

    let mut out = object.clone();
    if let Some(Value::Array(children)) = object.get("children") {
        let mut expanded = Vec::with_capacity(children.len());
        for child in children {
            match expand_node(child, owner_plugin, registry, stack, depth)? {
                Value::Array(many) => expanded.extend(many),
                one => expanded.push(one),
            }
        }
        if expanded.is_empty() {
            out.remove("children");
        } else {
            out.insert("children".into(), Value::Array(expanded));
        }
    }
    Ok(Value::Object(out))
}

fn expand_use(
    use_node: &Map<String, Value>,
    owner_plugin: &str,
    registry: &ComponentRegistry,
    stack: &mut Vec<String>,
    depth: usize,
) -> Result<Value, ComponentError> {
    if depth >= MAX_EXPAND_DEPTH {
        return Err(ComponentError::Expand {
            path: owner_plugin.into(),
            message: format!("component nesting exceeds {MAX_EXPAND_DEPTH} levels"),
        });
    }
    let component = use_node
        .get("component")
        .and_then(Value::as_str)
        .ok_or_else(|| ComponentError::Expand {
            path: owner_plugin.into(),
            message: "<use> needs a `component` attribute".into(),
        })?;
    let reference = ComponentRef::parse(component).map_err(|message| ComponentError::Expand {
        path: owner_plugin.into(),
        message,
    })?;
    let full = reference.full_name(owner_plugin);
    if stack.iter().any(|name| name == &full) {
        return Err(ComponentError::Expand {
            path: owner_plugin.into(),
            message: format!("component cycle involving `{full}`"),
        });
    }
    let definition = registry.get(&full).ok_or_else(|| ComponentError::Expand {
        path: owner_plugin.into(),
        message: format!("unknown component `{full}`"),
    })?;
    let body = definition
        .as_object()
        .and_then(|object| object.get("children"))
        .cloned()
        .unwrap_or(Value::Array(Vec::new()));

    let mut props = BTreeMap::new();
    for (key, value) in use_node {
        if key == "type" || key == "component" || key == "children" || key == "text" {
            continue;
        }
        let text = match value {
            Value::String(text) => text.clone(),
            Value::Bool(true) => "true".into(),
            Value::Bool(false) => "false".into(),
            Value::Number(number) => number.to_string(),
            _ => continue,
        };
        props.insert(key.clone(), text);
    }

    let mut default_slot = Vec::new();
    let mut named_slots: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    if let Some(Value::Array(children)) = use_node.get("children") {
        for child in children {
            let Some(child_object) = child.as_object() else {
                default_slot.push(child.clone());
                continue;
            };
            if child_object.get("type").and_then(Value::as_str) == Some("set") {
                let slot = child_object
                    .get("slot")
                    .and_then(Value::as_str)
                    .filter(|name| !name.is_empty())
                    .ok_or_else(|| ComponentError::Expand {
                        path: owner_plugin.into(),
                        message: format!("<set> in `{full}` needs a `slot` attribute"),
                    })?;
                let content = match child_object.get("children") {
                    Some(Value::Array(items)) => items.clone(),
                    _ => Vec::new(),
                };
                named_slots.insert(slot.to_string(), content);
            } else {
                default_slot.push(child.clone());
            }
        }
    }

    let filled = fill_slots(&body, &default_slot, &named_slots);
    let substituted = substitute_props(&filled, &props);

    stack.push(full.clone());
    let result = match substituted {
        Value::Array(items) => {
            let mut expanded = Vec::with_capacity(items.len());
            for item in items {
                match expand_node(&item, owner_plugin, registry, stack, depth + 1)? {
                    Value::Array(many) => expanded.extend(many),
                    one => expanded.push(one),
                }
            }
            if expanded.len() == 1 {
                expanded.pop().unwrap()
            } else {
                Value::Array(expanded)
            }
        }
        other => expand_node(&other, owner_plugin, registry, stack, depth + 1)?,
    };
    stack.pop();
    Ok(result)
}

fn fill_slots(
    body: &Value,
    default_slot: &[Value],
    named_slots: &BTreeMap<String, Vec<Value>>,
) -> Value {
    match body {
        Value::Array(items) => {
            let mut out = Vec::new();
            for item in items {
                match fill_slot_node(item, default_slot, named_slots) {
                    Value::Array(many) => out.extend(many),
                    one => out.push(one),
                }
            }
            Value::Array(out)
        }
        other => fill_slot_node(other, default_slot, named_slots),
    }
}

fn fill_slot_node(
    node: &Value,
    default_slot: &[Value],
    named_slots: &BTreeMap<String, Vec<Value>>,
) -> Value {
    let Some(object) = node.as_object() else {
        return node.clone();
    };
    if object.get("type").and_then(Value::as_str) == Some("slot") {
        let name = object.get("name").and_then(Value::as_str);
        let content = match name {
            None | Some("") => default_slot.to_vec(),
            Some(name) => named_slots.get(name).cloned().unwrap_or_default(),
        };
        return Value::Array(content);
    }

    let mut out = object.clone();
    if let Some(Value::Array(children)) = object.get("children") {
        let mut filled = Vec::with_capacity(children.len());
        for child in children {
            match fill_slot_node(child, default_slot, named_slots) {
                Value::Array(many) => filled.extend(many),
                one => filled.push(one),
            }
        }
        if filled.is_empty() {
            out.remove("children");
        } else {
            out.insert("children".into(), Value::Array(filled));
        }
    }
    Value::Object(out)
}

fn substitute_props(value: &Value, props: &BTreeMap<String, String>) -> Value {
    match value {
        Value::String(text) => Value::String(replace_props(text, props)),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| substitute_props(item, props))
                .collect(),
        ),
        Value::Object(object) => {
            let mut out = Map::new();
            for (key, child) in object {
                out.insert(key.clone(), substitute_props(child, props));
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

fn is_prop_name(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_lowercase())
        && characters.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Replace `{{name}}` with the prop value; unknown placeholders stay as written.
fn replace_props(text: &str, props: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'{'
            && index + 1 < bytes.len()
            && bytes[index + 1] == b'{'
            && let Some(end) = text[index + 2..].find("}}")
        {
            let name = &text[index + 2..index + 2 + end];
            if is_prop_name(name) {
                if let Some(value) = props.get(name) {
                    out.push_str(value);
                } else {
                    out.push_str(&text[index..index + 4 + end]);
                }
                index += 4 + end;
                continue;
            }
        }
        out.push(bytes[index] as char);
        index += 1;
    }
    out
}

/// Build a registry map from catalog rows: each entry is `plugin.name` → tree.
pub fn registry_from_documents(
    plugin: &str,
    components: &[ComponentDocument],
) -> ComponentRegistry {
    components
        .iter()
        .map(|component| {
            (
                format!("{plugin}.{}", component.name),
                component.tree.clone(),
            )
        })
        .collect()
}

/// One catalogued component row used to build a serve-time registry.
#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct CatalogComponentRow {
    pub full_name: String,
    pub component_tree: Value,
    pub plugin_name: String,
    pub plugin_version: String,
}

/// Keep only components belonging to the given installed `(name, version)` pairs.
pub fn registry_for_versions(
    rows: impl IntoIterator<Item = CatalogComponentRow>,
    installed: &[(String, String)],
) -> ComponentRegistry {
    rows.into_iter()
        .filter(|row| {
            installed
                .iter()
                .any(|(name, version)| name == &row.plugin_name && version == &row.plugin_version)
        })
        .map(|row| (row.full_name, row.component_tree))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_manager::pages::parse_page;

    fn component(xml: &str) -> Result<ComponentDocument, ComponentError> {
        parse_component(xml, Path::new("components/c.xml"))
    }

    #[test]
    fn parses_name_and_body() -> Result<(), ComponentError> {
        let document = component(
            r#"<component name="note_card">
  <group><field name="title" label="{{title}}" model="note" /></group>
</component>"#,
        )?;
        assert_eq!(document.name, "note_card");
        assert_eq!(document.tree["type"], "component");
        assert!(document.models.contains("note"));
        Ok(())
    }

    #[test]
    fn rejects_bad_names() {
        assert!(matches!(
            component(r#"<component name="NoteCard"><group/></component>"#),
            Err(ComponentError::InvalidName { .. })
        ));
        assert!(matches!(
            component(r#"<group name="x"/>"#),
            Err(ComponentError::NotAComponent(_))
        ));
        assert!(matches!(
            component(r#"<component><group/></component>"#),
            Err(ComponentError::MissingName(_))
        ));
    }

    #[test]
    fn expands_props_and_default_slot() -> Result<(), Box<dyn std::error::Error>> {
        let card = component(
            r#"<component name="note_card">
  <group>
    <field name="title" label="{{title}}" />
    <slot />
  </group>
</component>"#,
        )?;
        let page = parse_page(
            r#"<page route="/notes">
  <use component="note_card" title="Hello">
    <field name="body" label="Body" />
  </use>
</page>"#,
            Path::new("pages/notes.xml"),
        )?;
        let registry = registry_from_documents("notes", &[card]);
        let expanded = expand_tree(&page.tree, "notes", &registry)?;
        assert_eq!(expanded["type"], "page");
        let group = &expanded["children"][0];
        assert_eq!(group["type"], "group");
        assert_eq!(group["children"][0]["label"], "Hello");
        assert_eq!(group["children"][1]["name"], "body");
        Ok(())
    }

    #[test]
    fn expands_named_slots() -> Result<(), Box<dyn std::error::Error>> {
        let card = component(
            r#"<component name="card">
  <group>
    <slot name="actions" />
    <slot />
  </group>
</component>"#,
        )?;
        let page = parse_page(
            r#"<page route="/x">
  <use component="card">
    <field name="body" />
    <set slot="actions"><action name="go" label="Go" /></set>
  </use>
</page>"#,
            Path::new("pages/x.xml"),
        )?;
        let registry = registry_from_documents("p", &[card]);
        let expanded = expand_tree(&page.tree, "p", &registry)?;
        let group = &expanded["children"][0];
        assert_eq!(group["children"][0]["type"], "action");
        assert_eq!(group["children"][1]["name"], "body");
        Ok(())
    }

    #[test]
    fn detects_cycles() -> Result<(), Box<dyn std::error::Error>> {
        let a = component(r#"<component name="a"><use component="b" /></component>"#)?;
        let b = component(r#"<component name="b"><use component="a" /></component>"#)?;
        let page = parse_page(
            r#"<page route="/x"><use component="a" /></page>"#,
            Path::new("pages/x.xml"),
        )?;
        let registry = registry_from_documents("p", &[a, b]);
        let error = expand_tree(&page.tree, "p", &registry).unwrap_err();
        assert!(matches!(error, ComponentError::Expand { .. }));
        assert!(error.to_string().contains("cycle"));
        Ok(())
    }

    #[test]
    fn resolves_qualified_names() -> Result<(), Box<dyn std::error::Error>> {
        let card = component(
            r#"<component name="note_card"><field name="t" label="{{title}}" /></component>"#,
        )?;
        let page = parse_page(
            r#"<page route="/x"><use component="notes.note_card" title="Hi" /></page>"#,
            Path::new("pages/x.xml"),
        )?;
        let mut registry = ComponentRegistry::new();
        registry.insert("notes.note_card".into(), card.tree);
        let expanded = expand_tree(&page.tree, "crm", &registry)?;
        assert_eq!(expanded["children"][0]["label"], "Hi");
        Ok(())
    }

    #[test]
    fn collect_uses_finds_references() -> Result<(), Box<dyn std::error::Error>> {
        let page = parse_page(
            r#"<page route="/x">
  <use component="local" />
  <use component="other.remote" />
</page>"#,
            Path::new("pages/x.xml"),
        )?;
        let uses = collect_uses(&page.tree);
        assert_eq!(uses.len(), 2);
        assert_eq!(uses[0].0.name, "local");
        assert_eq!(uses[1].0.plugin.as_deref(), Some("other"));
        Ok(())
    }

    #[test]
    fn wrap_page_fills_default_slot_and_title() -> Result<(), Box<dyn std::error::Error>> {
        let shell = component(
            r#"<component name="shell">
  <group title="{{title}}">
    <slot />
  </group>
</component>"#,
        )?;
        let page = parse_page(
            r#"<page route="/board" title="Board" wrap="shell">
  <field name="body" label="Body" />
</page>"#,
            Path::new("pages/board.xml"),
        )?;
        assert_eq!(page.wrap.as_deref(), Some("shell"));
        let wrapped = wrap_page_tree(&page.tree, "shell", &page.title)?;
        assert!(wrapped.get("wrap").is_none());
        let registry = registry_from_documents("notes", &[shell]);
        let expanded = expand_tree(&wrapped, "notes", &registry)?;
        let group = &expanded["children"][0];
        assert_eq!(group["type"], "group");
        assert_eq!(group["title"], "Board");
        assert_eq!(group["children"][0]["name"], "body");
        Ok(())
    }

    #[test]
    fn wrap_page_supports_named_slots() -> Result<(), Box<dyn std::error::Error>> {
        let shell = component(
            r#"<component name="shell">
  <group>
    <slot />
    <slot name="aside" />
  </group>
</component>"#,
        )?;
        let page = parse_page(
            r#"<page route="/x" title="X" wrap="shell">
  <field name="main" />
  <set slot="aside"><stat label="Tip" value="Hi" /></set>
</page>"#,
            Path::new("pages/x.xml"),
        )?;
        let wrapped = wrap_page_tree(&page.tree, page.wrap.as_deref().unwrap(), &page.title)?;
        let registry = registry_from_documents("p", &[shell]);
        let expanded = expand_tree(&wrapped, "p", &registry)?;
        let group = &expanded["children"][0];
        assert_eq!(group["children"][0]["name"], "main");
        assert_eq!(group["children"][1]["type"], "stat");
        Ok(())
    }

    #[test]
    fn wrap_page_rejects_unknown_component_on_expand() -> Result<(), Box<dyn std::error::Error>> {
        let page = parse_page(
            r#"<page route="/x" wrap="missing"><field name="a" /></page>"#,
            Path::new("pages/x.xml"),
        )?;
        let wrapped = wrap_page_tree(&page.tree, "missing", "X")?;
        let registry = ComponentRegistry::new();
        let error = expand_tree(&wrapped, "p", &registry).unwrap_err();
        assert!(error.to_string().contains("unknown component"));
        Ok(())
    }

    #[tokio::test]
    async fn discovers_components_and_rejects_duplicates() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = tempfile::tempdir()?;
        let package = root.path();
        tokio::fs::create_dir_all(package.join("components/cards")).await?;
        tokio::fs::write(
            package.join("components/cards/note.xml"),
            r#"<component name="note_card"><group/></component>"#,
        )
        .await?;
        let found = discover_components(package).await?;
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "note_card");

        tokio::fs::write(
            package.join("components/again.xml"),
            r#"<component name="note_card"><group/></component>"#,
        )
        .await?;
        assert!(matches!(
            discover_components(package).await,
            Err(ComponentError::DuplicateName { .. })
        ));
        Ok(())
    }
}
