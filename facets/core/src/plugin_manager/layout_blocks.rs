//! Composed layouts: a layout definition that lists regions and the blocks in them.
//!
//! ```json
//! {
//!   "name": "sea", "label": "Sea",
//!   "regions": {
//!     "topbar":  { "tone": "default", "blocks": [
//!         { "type": "home-menu" }, { "type": "app-title" }, { "type": "spacer" },
//!         { "type": "notifications" }, { "type": "user-menu" } ] },
//!     "sidebar": { "width": "15rem", "blocks": [ { "type": "app-menu" } ] },
//!     "footer":  { "blocks": [ { "type": "text", "props": { "text": "© Sea Ltd" } } ] }
//!   }
//! }
//! ```
//!
//! Every block may also carry `id`, `when` (`user`, `developer`, `manager`, `admin`),
//! `roles` (role names, any of which shows it) and `hide_below` (a breakpoint). The web
//! app renders the blocks with the components in `web/src/lib/components/layout/composed`;
//! a type the web app does not know is rejected here so a theme cannot fail at runtime.

use serde_json::{Map, Value};

use super::themes::{is_identifier, is_safe_href};

pub const REGIONS: &[&str] = &["banner", "topbar", "sidebar", "aside", "footer"];

pub const BLOCK_TYPES: &[&str] = &[
    "app-menu",
    "app-title",
    "brand",
    "color-mode",
    "company-switcher",
    "developer-badge",
    "divider",
    "home-menu",
    "link",
    "nav",
    "notifications",
    "org-switcher",
    "spacer",
    "text",
    "user-menu",
];

const WHEN: &[&str] = &["user", "developer", "manager", "admin"];
const BREAKPOINTS: &[&str] = &["sm", "md", "lg", "xl"];
const TONES: &[&str] = &["default", "muted", "primary", "inverted"];
const NAV_SOURCES: &[&str] = &["theme", "desk", "manage"];
const ORIENTATIONS: &[&str] = &["horizontal", "vertical"];

const MAX_BLOCKS_PER_REGION: usize = 40;
const MAX_TEXT: usize = 200;
const MAX_ROLES: usize = 20;

/// Check a layout's `regions` object.
pub fn validate_regions(regions: &Value) -> Result<(), String> {
    let regions = regions.as_object().ok_or("`regions` must be an object")?;
    if regions.is_empty() {
        return Err("`regions` must name at least one region".into());
    }
    for (name, region) in regions {
        if !REGIONS.contains(&name.as_str()) {
            return Err(format!(
                "unknown region `{name}`; regions are {}",
                REGIONS.join(", ")
            ));
        }
        validate_region(region).map_err(|reason| format!("region `{name}`: {reason}"))?;
    }
    Ok(())
}

fn validate_region(region: &Value) -> Result<(), String> {
    let region = region.as_object().ok_or("must be an object")?;
    for key in region.keys() {
        if !["blocks", "width", "position", "tone", "sticky"].contains(&key.as_str()) {
            return Err(format!("unknown key `{key}`"));
        }
    }
    if let Some(width) = region.get("width") {
        let width = width.as_str().ok_or("`width` must be a string")?;
        if !is_css_length(width) {
            return Err("`width` must be a length such as `15rem` or `240px`".into());
        }
    }
    one_of(region, "position", &["left", "right"])?;
    one_of(region, "tone", TONES)?;
    if let Some(sticky) = region.get("sticky")
        && !sticky.is_boolean()
    {
        return Err("`sticky` must be true or false".into());
    }
    let blocks = region
        .get("blocks")
        .and_then(Value::as_array)
        .ok_or("`blocks` must be an array")?;
    if blocks.len() > MAX_BLOCKS_PER_REGION {
        return Err(format!("more than {MAX_BLOCKS_PER_REGION} blocks"));
    }
    for (index, block) in blocks.iter().enumerate() {
        validate_block(block).map_err(|reason| format!("block {index}: {reason}"))?;
    }
    Ok(())
}

fn validate_block(block: &Value) -> Result<(), String> {
    let block = block.as_object().ok_or("must be an object")?;
    for key in block.keys() {
        if !["type", "id", "props", "when", "roles", "hide_below"].contains(&key.as_str()) {
            return Err(format!("unknown key `{key}`"));
        }
    }
    let kind = block
        .get("type")
        .and_then(Value::as_str)
        .ok_or("`type` is required")?;
    if !BLOCK_TYPES.contains(&kind) {
        return Err(format!(
            "unknown block type `{kind}`; types are {}",
            BLOCK_TYPES.join(", ")
        ));
    }
    if let Some(id) = block.get("id")
        && !id.as_str().is_some_and(is_identifier)
    {
        return Err("`id` must be lowercase letters, digits, `-` and `_`".into());
    }
    one_of(block, "when", WHEN)?;
    one_of(block, "hide_below", BREAKPOINTS)?;
    if let Some(roles) = block.get("roles") {
        let roles = roles.as_array().ok_or("`roles` must be an array")?;
        if roles.len() > MAX_ROLES
            || !roles
                .iter()
                .all(|role| role.as_str().is_some_and(is_identifier))
        {
            return Err(format!("`roles` must be up to {MAX_ROLES} role names"));
        }
    }
    let empty = Map::new();
    let props = match block.get("props") {
        None => &empty,
        Some(Value::Object(props)) => props,
        Some(_) => return Err("`props` must be an object".into()),
    };
    validate_props(kind, props).map_err(|reason| format!("`{kind}` props: {reason}"))
}

fn validate_props(kind: &str, props: &Map<String, Value>) -> Result<(), String> {
    let allowed: &[&str] = match kind {
        "brand" => &["label", "show_logo", "href"],
        "link" => &["label", "href", "icon"],
        "text" => &["text", "muted"],
        "nav" => &["source", "orientation"],
        "app-menu" => &["source"],
        "home-menu" | "app-title" => &["label"],
        _ => &[],
    };
    for (key, value) in props {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("unknown prop `{key}`"));
        }
        match value {
            Value::String(text) if text.chars().count() > MAX_TEXT => {
                return Err(format!("`{key}` is longer than {MAX_TEXT} characters"));
            }
            Value::String(_) | Value::Bool(_) => {}
            _ => return Err(format!("`{key}` must be a string or a boolean")),
        }
    }
    if let Some(href) = props.get("href") {
        let href = href.as_str().ok_or("`href` must be a string")?;
        if !is_safe_href(href) {
            return Err("`href` must be an app path starting with `/` or an http(s) URL".into());
        }
    }
    one_of(props, "source", NAV_SOURCES)?;
    one_of(props, "orientation", ORIENTATIONS)?;
    if kind == "link" {
        for required in ["label", "href"] {
            if !props
                .get(required)
                .and_then(Value::as_str)
                .is_some_and(|v| !v.trim().is_empty())
            {
                return Err(format!("`{required}` is required"));
            }
        }
    }
    if kind == "text" && !props.get("text").is_some_and(Value::is_string) {
        return Err("`text` is required".into());
    }
    Ok(())
}

fn one_of(object: &Map<String, Value>, key: &str, choices: &[&str]) -> Result<(), String> {
    match object.get(key) {
        None => Ok(()),
        Some(Value::String(value)) if choices.contains(&value.as_str()) => Ok(()),
        Some(_) => Err(format!("`{key}` must be one of {}", choices.join(", "))),
    }
}

/// `12rem`, `240px`, `18em`, `20%`: a positive number and a unit, nothing that could
/// escape the CSS property it is written into.
fn is_css_length(value: &str) -> bool {
    let unit_at = value
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(value.len());
    let (number, unit) = value.split_at(unit_at);
    !number.is_empty()
        && number.len() <= 6
        && number.parse::<f64>().is_ok_and(|n| n > 0.0)
        && ["px", "rem", "em", "%", "ch"].contains(&unit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accepts_a_composed_layout() {
        let regions = json!({
            "topbar": { "tone": "primary", "sticky": true, "blocks": [
                { "type": "home-menu" },
                { "type": "brand", "props": { "label": "Sea", "show_logo": true } },
                { "type": "spacer" },
                { "type": "link", "props": { "label": "Help", "href": "https://help.example" }, "hide_below": "md" },
                { "type": "developer-badge", "when": "developer" },
                { "type": "user-menu", "roles": ["org_admin"] }
            ] },
            "sidebar": { "width": "15rem", "position": "right", "blocks": [ { "type": "app-menu", "props": { "source": "desk" } } ] },
            "footer": { "blocks": [ { "type": "text", "props": { "text": "© Sea" } } ] }
        });
        assert_eq!(validate_regions(&regions), Ok(()));
    }

    #[test]
    fn rejects_bad_layouts() {
        let bad = [
            json!([]),
            json!({}),
            json!({ "header": { "blocks": [] } }),
            json!({ "topbar": { "blocks": "x" } }),
            json!({ "topbar": { "blocks": [], "color": "red" } }),
            json!({ "topbar": { "blocks": [ { "type": "iframe" } ] } }),
            json!({ "topbar": { "blocks": [ { "props": {} } ] } }),
            json!({ "topbar": { "blocks": [ { "type": "spacer", "props": { "x": 1 } } ] } }),
            json!({ "topbar": { "blocks": [ { "type": "link", "props": { "label": "x", "href": "javascript:alert(1)" } } ] } }),
            json!({ "topbar": { "blocks": [ { "type": "link", "props": { "label": "x" } } ] } }),
            json!({ "topbar": { "blocks": [ { "type": "text" } ] } }),
            json!({ "topbar": { "blocks": [ { "type": "spacer", "when": "everyone" } ] } }),
            json!({ "topbar": { "blocks": [ { "type": "spacer", "roles": ["Not A Role"] } ] } }),
            json!({ "sidebar": { "width": "1px; color: red", "blocks": [] } }),
            json!({ "sidebar": { "width": "calc(100%)", "blocks": [] } }),
            json!({ "sidebar": { "tone": "neon", "blocks": [] } }),
        ];
        for regions in bad {
            assert!(validate_regions(&regions).is_err(), "{regions}");
        }
    }

    #[test]
    fn limits_blocks_per_region() {
        let blocks: Vec<Value> = (0..41).map(|_| json!({ "type": "spacer" })).collect();
        assert!(validate_regions(&json!({ "topbar": { "blocks": blocks } })).is_err());
    }

    #[test]
    fn css_lengths() {
        for good in ["15rem", "240px", "20%", "1.5em", "30ch"] {
            assert!(is_css_length(good), "{good}");
        }
        for bad in ["", "rem", "0px", "-1px", "10", "10vw", "1px;", "1e3px"] {
            assert!(!is_css_length(bad), "{bad}");
        }
    }
}
