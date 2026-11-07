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

pub const REGIONS: &[&str] = &[
    "banner", "topbar", "subbar", "sidebar", "aside", "bottombar", "footer",
];

pub const BLOCK_TYPES: &[&str] = &[
    "action-bar",
    "app-menu",
    "app-title",
    "back",
    "brand",
    "breadcrumbs",
    "color-mode",
    "company-switcher",
    "create",
    "developer-badge",
    "divider",
    "home-menu",
    "link",
    "nav",
    "notifications",
    "org-switcher",
    "pinned",
    "save-bar",
    "scope-switcher",
    "search",
    "sidebar-toggle",
    "skip-link",
    "spacer",
    "text",
    "user-menu",
    "workspace-tabs",
];

const WHEN: &[&str] = &["user", "developer", "manager", "admin"];
const BREAKPOINTS: &[&str] = &["sm", "md", "lg", "xl"];
const TONES: &[&str] = &["default", "muted", "primary", "inverted"];
const NAV_SOURCES: &[&str] = &["theme", "desk", "manage", "footer"];
const ORIENTATIONS: &[&str] = &["horizontal", "vertical"];
/// How `nav` and `app-menu` draw their items.
/// How the `home-menu` opens: a side panel, a dropdown under the button, a full-width launcher or a narrow drawer.
pub const MENU_VARIANTS: &[&str] = &["panel", "dropdown", "launcher", "drawer"];
/// What the `home-menu` button looks like.
pub const MENU_TRIGGERS: &[&str] = &["grid", "hamburger", "label"];

pub const NAV_VARIANTS: &[&str] = &["links", "tree", "tabs", "mega", "columns", "tiles"];
/// Which groups of a `tree` stay open.
const NAV_EXPANDS: &[&str] = &["toggle", "active", "all"];
/// How `breadcrumbs` separates its crumbs.
const CRUMB_STYLES: &[&str] = &["chevron", "slash", "arrows"];
/// How `search` looks, and the extra key that opens it.
const SEARCH_STYLES: &[&str] = &["field", "button", "icon"];
const SEARCH_SHORTCUTS: &[&str] = &["k", "/"];

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
        if !["blocks", "width", "position", "tone", "sticky", "mode", "collapsed_width", "overlay", "show_below"].contains(&key.as_str()) {
            return Err(format!("unknown key `{key}`"));
        }
    }
    if let Some(width) = region.get("width") {
        let width = width.as_str().ok_or("`width` must be a string")?;
        if !is_css_length(width) {
            return Err("`width` must be a length such as `15rem` or `240px`".into());
        }
    }
    if let Some(width) = region.get("collapsed_width") {
        let width = width.as_str().ok_or("`collapsed_width` must be a string")?;
        if !is_css_length(width) {
            return Err("`collapsed_width` must be a length such as `3.5rem`".into());
        }
    }
    // `rail` keeps a narrow strip of icons; `collapsible` lets the person fold the sidebar away.
    one_of(region, "mode", &["full", "rail", "collapsible"])?;
    one_of(region, "show_below", BREAKPOINTS)?;
    if let Some(overlay) = region.get("overlay")
        && !overlay.is_boolean()
    {
        return Err("`overlay` must be true or false".into());
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
        if !["type", "id", "props", "items", "when", "roles", "hide_below", "show_below"].contains(&key.as_str()) {
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
    if let Some(items) = block.get("items") {
        // Links and groups written inside a `nav` or `app-menu` block replace what it would read.
        if !["nav", "app-menu"].contains(&kind) {
            return Err(format!("`{kind}` blocks have no `items`"));
        }
        let items = items.as_array().ok_or("`items` must be an array")?;
        let mut count = 0;
        super::themes::validate_items(items, 1, &mut count).map_err(|reason| format!("`items`: {reason}"))?;
    }
    one_of(block, "when", WHEN)?;
    one_of(block, "hide_below", BREAKPOINTS)?;
    one_of(block, "show_below", BREAKPOINTS)?;
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
        "brand" => &["label", "show_logo", "href", "follow"],
        "link" => &["label", "href", "icon"],
        "text" => &["text", "muted"],
        "nav" => &["source", "orientation", "variant", "expand", "overflow", "more_label"],
        "app-menu" => &["source", "variant", "expand", "overflow", "more_label"],
        "breadcrumbs" => &["style", "scope"],
        "search" => &["style", "shortcut"],
        "back" => &["label"],
        "create" => &["label", "style"],
        "pinned" => &["label"],
        "skip-link" => &["label", "target"],
        "workspace-tabs" => &["limit", "pin"],
        "scope-switcher" => &["source", "label"],
        "home-menu" => &["label", "variant", "trigger"],
        "app-title" => &["label"],
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
    if kind == "create" {
        // `button` sits in a bar; `fab` floats over the page.
        one_of(props, "style", &["button", "fab"])?;
        return Ok(());
    }
    if kind == "skip-link" {
        // The id of the element focus jumps to; the page's content by default.
        if let Some(target) = props.get("target").and_then(Value::as_str)
            && !is_identifier(target)
        {
            return Err("`target` must be an element id such as `content`".into());
        }
        return Ok(());
    }
    if kind == "workspace-tabs" {
        if let Some(limit) = props.get("limit").and_then(Value::as_str)
            && !limit.parse::<u8>().is_ok_and(|n| (2..=20).contains(&n))
        {
            return Err("`limit` must be a number from 2 to 20".into());
        }
        return Ok(());
    }
    if matches!(kind, "app-menu" | "nav")
        && let Some(overflow) = props.get("overflow").and_then(Value::as_str)
        && overflow != "auto"
        && !overflow.parse::<u8>().is_ok_and(|n| (2..=20).contains(&n))
    {
        return Err("`overflow` must be `auto` or a number from 2 to 20".into());
    }
    if kind == "brand" {
        // `app`: the brand shows the open app's name while one is open.
        one_of(props, "follow", &["none", "app"])?;
    }
    if kind == "home-menu" {
        one_of(props, "variant", MENU_VARIANTS)?;
        one_of(props, "trigger", MENU_TRIGGERS)?;
        return Ok(());
    }
    if kind == "scope-switcher" {
        // The scopes come from a function: `plugin.function`.
        let source = props
            .get("source")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !source.contains('.') || !super::pages::is_function_ref(source) {
            return Err(
                "`source` is required: the function that lists the scopes, `plugin.function`"
                    .into(),
            );
        }
    } else {
        one_of(props, "source", NAV_SOURCES)?;
    }
    if kind == "app-menu" && props.get("source").and_then(Value::as_str) == Some("footer") {
        return Err("`source` \"footer\" is only for `nav` blocks".into());
    }
    one_of(props, "orientation", ORIENTATIONS)?;
    one_of(props, "variant", NAV_VARIANTS)?;
    one_of(props, "expand", NAV_EXPANDS)?;
    match kind {
        "search" => {
            one_of(props, "style", SEARCH_STYLES)?;
            one_of(props, "shortcut", SEARCH_SHORTCUTS)?;
        }
        _ => one_of(props, "style", CRUMB_STYLES)?,
    }
    if props.get("scope").is_some_and(|scope| !scope.is_boolean()) {
        return Err("`scope` must be true or false".into());
    }
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
            "sidebar": { "width": "15rem", "position": "right", "blocks": [ { "type": "app-menu", "props": { "source": "desk", "variant": "tree", "expand": "active" } } ] },
            "subbar": { "tone": "muted", "blocks": [ { "type": "app-menu", "props": { "variant": "tabs" } } ] },
            "banner": { "blocks": [ { "type": "breadcrumbs", "props": { "style": "arrows", "scope": true } }, { "type": "search", "props": { "style": "button", "shortcut": "/" } }, { "type": "nav", "props": { "source": "footer", "variant": "columns" } }, { "type": "back", "props": { "label": "Back" } }, { "type": "scope-switcher", "props": { "source": "crm.projects", "label": "Project" } }, { "type": "nav", "props": { "variant": "tabs" } }, { "type": "nav", "props": { "variant": "mega", "orientation": "horizontal" } } ] },
            "footer": { "blocks": [ { "type": "text", "props": { "text": "© Sea" } } ] }
        });
        assert_eq!(validate_regions(&regions), Ok(()));
    }

    #[test]
    fn home_menu_variants_are_checked() {
        let ok = json!({ "topbar": { "blocks": [ { "type": "home-menu", "props": { "variant": "launcher", "trigger": "hamburger" } }, { "type": "brand", "props": { "follow": "app" } } ] } });
        assert_eq!(validate_regions(&ok), Ok(()));
        for props in [json!({ "variant": "mega" }), json!({ "trigger": "dots" })] {
            let bad = json!({ "topbar": { "blocks": [ { "type": "home-menu", "props": props } ] } });
            assert!(validate_regions(&bad).is_err());
        }
    }

    #[test]
    fn shell_regions_and_blocks_are_checked() {
        let ok = json!({
            "sidebar": { "mode": "rail", "collapsed_width": "3.5rem", "overlay": true, "blocks": [ { "type": "sidebar-toggle" }, { "type": "pinned" } ] },
            "bottombar": { "show_below": "md", "blocks": [ { "type": "create", "props": { "style": "fab" } } ] },
            "topbar": { "blocks": [ { "type": "skip-link", "props": { "target": "content" } }, { "type": "workspace-tabs", "props": { "limit": "8" } }, { "type": "app-menu", "props": { "variant": "mega", "overflow": "6", "more_label": "More" } }, { "type": "save-bar" }, { "type": "action-bar", "show_below": "lg" } ] }
        });
        assert_eq!(validate_regions(&ok), Ok(()));
        for bad in [
            json!({ "sidebar": { "mode": "wide", "blocks": [] } }),
            json!({ "sidebar": { "collapsed_width": "wide", "blocks": [] } }),
            json!({ "bottombar": { "show_below": "huge", "blocks": [] } }),
            json!({ "topbar": { "blocks": [ { "type": "create", "props": { "style": "star" } } ] } }),
            json!({ "topbar": { "blocks": [ { "type": "workspace-tabs", "props": { "limit": "99" } } ] } }),
            json!({ "topbar": { "blocks": [ { "type": "skip-link", "props": { "target": "a b" } } ] } }),
        ] {
            assert!(validate_regions(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn nav_blocks_can_carry_items() {
        let ok = json!({ "topbar": { "blocks": [ { "type": "nav", "items": [
            { "label": "Home", "href": "/apps", "icon": "home" },
            { "label": "Sales", "children": [ { "label": "Deals", "href": "/crm/deals" } ] }
        ] } ] } });
        assert_eq!(validate_regions(&ok), Ok(()));
        for bad in [
            json!({ "topbar": { "blocks": [ { "type": "text", "items": [] } ] } }),
            json!({ "topbar": { "blocks": [ { "type": "nav", "items": [ { "label": "", "href": "/x" } ] } ] } }),
            json!({ "topbar": { "blocks": [ { "type": "nav", "items": [ { "label": "x", "href": "javascript:alert(1)" } ] } ] } }),
            json!({ "topbar": { "blocks": [ { "type": "nav", "items": "all" } ] } }),
        ] {
            assert!(validate_regions(&bad).is_err(), "{bad}");
        }
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
            json!({ "topbar": { "blocks": [ { "type": "nav", "props": { "variant": "carousel" } } ] } }),
            json!({ "footer": { "blocks": [ { "type": "app-menu", "props": { "source": "footer" } } ] } }),
            json!({ "topbar": { "blocks": [ { "type": "scope-switcher" } ] } }),
            json!({ "topbar": { "blocks": [ { "type": "scope-switcher", "props": { "source": "projects" } } ] } }),
            json!({ "topbar": { "blocks": [ { "type": "scope-switcher", "props": { "source": "x.y.z" } } ] } }),
            json!({ "topbar": { "blocks": [ { "type": "back", "props": { "variant": "x" } } ] } }),
            json!({ "topbar": { "blocks": [ { "type": "breadcrumbs", "props": { "style": "dots" } } ] } }),
            json!({ "topbar": { "blocks": [ { "type": "search", "props": { "style": "arrows" } } ] } }),
            json!({ "topbar": { "blocks": [ { "type": "search", "props": { "shortcut": "ctrl+f" } } ] } }),
            json!({ "topbar": { "blocks": [ { "type": "breadcrumbs", "props": { "scope": "yes" } } ] } }),
            json!({ "topbar": { "blocks": [ { "type": "app-menu", "props": { "expand": "some" } } ] } }),
            json!({ "topbar": { "blocks": [ { "type": "app-menu", "props": { "overflow": "1" } } ] } }),
            json!({ "topbar": { "blocks": [ { "type": "link", "props": { "label": "x", "href": "/x", "variant": "tabs" } } ] } }),
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
