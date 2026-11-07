//! Layouts written as XML (`layouts/<name>.xml`), turned into the same JSON the `.json`
//! layouts use and checked by the same code (`layout_blocks`).
//!
//! ```xml
//! <layout name="navbar" label="Navbar">
//!   <topbar sticky="true">
//!     <home-menu variant="launcher"/>
//!     <app-title/>
//!     <app-menu variant="links"/>
//!     <spacer/>
//!     <user-menu when="user" hide_below="md"/>
//!   </topbar>
//!   <footer><text muted="true">© Sea Ltd</text></footer>
//! </layout>
//! ```
//!
//! A region is an element named after it; its attributes are the region's keys and its
//! children are the blocks. A block is an element named after its type; `id`, `when`,
//! `hide_below` and `roles` (comma separated) are the block's own keys, every other
//! attribute is a prop, and the text of a `<text>` block is its `text` prop.

use std::path::Path;

use serde_json::{Map, Value};

use super::xml_tree::{Node, compile_xml};

/// The layout JSON for an XML layout file.
pub fn layout_from_xml(xml: &str, source: &Path) -> Result<Value, String> {
    let root = compile_xml(xml, source).map_err(|error| error.to_string())?;
    if root.tag != "layout" {
        return Err(format!("the root element must be `<layout>`, not `<{}>`", root.tag));
    }
    let mut layout = Map::new();
    let mut regions = Map::new();
    for (key, value) in &root.attributes {
        match key.as_str() {
            "name" | "label" | "chrome" => {
                layout.insert(key.clone(), Value::String(value.clone()));
            }
            _ => return Err(format!("`<layout>` has no attribute `{key}`")),
        }
    }
    for child in root.children {
        let tag = child.tag.clone();
        if regions.contains_key(&tag) {
            return Err(format!("region `{tag}` appears twice"));
        }
        regions.insert(tag.clone(), region(child).map_err(|reason| format!("`<{tag}>`: {reason}"))?);
    }
    if !regions.is_empty() {
        layout.insert("regions".into(), Value::Object(regions));
    }
    Ok(Value::Object(layout))
}

fn scalar(value: &str) -> Value {
    match value {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => Value::String(value.to_string()),
    }
}

fn region(node: Node) -> Result<Value, String> {
    if node.text.is_some() {
        return Err("holds text; put blocks inside it".into());
    }
    let mut region = Map::new();
    for (key, value) in &node.attributes {
        region.insert(key.clone(), scalar(value));
    }
    let blocks = node.children.into_iter().map(block).collect::<Result<Vec<_>, _>>()?;
    region.insert("blocks".into(), Value::Array(blocks));
    Ok(Value::Object(region))
}

/// `<link label href icon description badge badge_function/>` and `<group label icon href>` become
/// nav items; a group holds links and groups (the nesting limit is checked with the rest).
fn nav_item(node: Node) -> Result<Value, String> {
    let tag = node.tag.clone();
    if node.text.is_some() {
        return Err(format!("`<{tag}>` cannot hold text"));
    }
    let mut item = Map::new();
    for (key, value) in &node.attributes {
        match key.as_str() {
            "label" | "href" | "icon" | "description" if tag == "link" || tag == "group" => {
                item.insert(key.clone(), Value::String(value.clone()));
            }
            // A short text, or `badge_function="plugin.fn"` for a live count.
            "badge" => {
                item.insert("badge".into(), Value::String(value.clone()));
            }
            "badge_function" => {
                item.insert("badge".into(), serde_json::json!({ "function": value }));
            }
            _ => return Err(format!("`<{tag}>` has no attribute `{key}`")),
        }
    }
    match tag.as_str() {
        "link" if !node.children.is_empty() => {
            return Err("`<link>` cannot hold other elements; use `<group>`".into());
        }
        "link" => {}
        "group" => {
            let children = node.children.into_iter().map(nav_item).collect::<Result<Vec<_>, _>>()?;
            item.insert("children".into(), Value::Array(children));
        }
        other => return Err(format!("`<{other}>` is not a nav item; use `<link>` or `<group>`")),
    }
    Ok(Value::Object(item))
}

fn block(node: Node) -> Result<Value, String> {
    let tag = node.tag.clone();
    let mut items = None;
    let node = if matches!(tag.as_str(), "nav" | "app-menu") && !node.children.is_empty() {
        let mut node = node;
        let children = std::mem::take(&mut node.children);
        items = Some(children.into_iter().map(nav_item).collect::<Result<Vec<_>, _>>()?);
        node
    } else {
        node
    };
    if !node.children.is_empty() {
        return Err(format!("`<{tag}>` cannot hold other elements"));
    }
    let mut block = Map::new();
    let mut props = Map::new();
    for (key, value) in &node.attributes {
        match key.as_str() {
            "id" | "when" | "hide_below" => {
                block.insert(key.clone(), Value::String(value.clone()));
            }
            "roles" => {
                let roles = value.split(',').map(str::trim).filter(|role| !role.is_empty());
                block.insert("roles".into(), Value::Array(roles.map(|r| Value::String(r.into())).collect()));
            }
            _ => {
                props.insert(key.clone(), scalar(value));
            }
        }
    }
    if let Some(text) = node.text {
        if tag != "text" {
            return Err(format!("`<{tag}>` cannot hold text"));
        }
        props.insert("text".into(), Value::String(text));
    }
    block.insert("type".into(), Value::String(tag));
    if let Some(items) = items {
        block.insert("items".into(), Value::Array(items));
    }
    if !props.is_empty() {
        block.insert("props".into(), Value::Object(props));
    }
    Ok(Value::Object(block))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_regions_blocks_and_props() {
        let xml = r#"<layout name="navbar" label="Navbar">
            <topbar sticky="true" tone="primary">
              <home-menu variant="launcher" trigger="grid"/>
              <spacer/>
              <user-menu roles="org_admin, developer" hide_below="md"/>
            </topbar>
            <footer><text muted="true">© Sea</text></footer>
        </layout>"#;
        let value = layout_from_xml(xml, Path::new("layouts/navbar.xml")).unwrap();
        assert_eq!(value["name"], "navbar");
        assert_eq!(value["regions"]["topbar"]["sticky"], true);
        assert_eq!(value["regions"]["topbar"]["blocks"][0]["props"]["variant"], "launcher");
        assert_eq!(value["regions"]["topbar"]["blocks"][2]["roles"], json!(["org_admin", "developer"]));
        assert_eq!(value["regions"]["footer"]["blocks"][0]["props"]["text"], "© Sea");
        assert_eq!(super::super::layout_blocks::validate_regions(&value["regions"]), Ok(()));
    }

    #[test]
    fn navigation_is_written_inside_the_block() {
        let xml = r#"<layout name="n"><topbar>
            <nav variant="mega">
              <link label="Home" href="/apps" icon="home"/>
              <group label="Sales" icon="tag">
                <link label="Deals" href="/crm/deals" badge="3"/>
                <link label="Tickets" href="/helpdesk" badge_function="helpdesk.open_tickets"/>
              </group>
            </nav>
        </topbar></layout>"#;
        let value = layout_from_xml(xml, Path::new("n.xml")).unwrap();
        let block = &value["regions"]["topbar"]["blocks"][0];
        assert_eq!(block["items"][0]["label"], "Home");
        assert_eq!(block["items"][1]["children"][0]["badge"], "3");
        assert_eq!(block["items"][1]["children"][1]["badge"]["function"], "helpdesk.open_tickets");
        assert_eq!(block["props"]["variant"], "mega");
        assert_eq!(super::super::layout_blocks::validate_regions(&value["regions"]), Ok(()));
        for bad in [
            "<layout><topbar><spacer><link label='a' href='/'/></spacer></topbar></layout>",
            "<layout><topbar><nav><button/></nav></topbar></layout>",
            "<layout><topbar><nav><link label='a' href='/'><link label='b' href='/'/></link></nav></topbar></layout>",
            "<layout><topbar><nav><link label='a' colour='red'/></nav></topbar></layout>",
        ] {
            assert!(layout_from_xml(bad, Path::new("n.xml")).is_err(), "{bad}");
        }
    }

    #[test]
    fn refuses_what_it_cannot_map() {
        for xml in [
            "<page/>",
            r#"<layout name="a" colour="red"/>"#,
            "<layout><topbar><spacer><spacer/></spacer></topbar></layout>",
            "<layout><topbar>words</topbar></layout>",
            "<layout><topbar/><topbar/></layout>",
            "<layout><topbar><link>words</link></topbar></layout>",
        ] {
            assert!(layout_from_xml(xml, Path::new("x.xml")).is_err(), "{xml}");
        }
    }
}
