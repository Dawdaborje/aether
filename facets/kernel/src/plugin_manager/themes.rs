//! Theme plugins.
//!
//! A theme plugin declares `[theme]` in `plugin.toml` and points `tokens_file`
//! at a JSON file. That file holds the design tokens and also how the app is
//! laid out:
//!
//! ```json
//! {
//!   "name": "ocean", "label": "Ocean", "color_mode": "system",
//!   "radius": "0.375rem", "font_sans": "…",
//!   "light": { "background": "…" }, "dark": { "background": "…" },
//!   "layout": "custom",
//!   "error_pages": "custom",
//!   "nav": { "header": "Ocean", "items": [
//!       { "label": "Apps", "href": "/apps", "icon": "layout-dashboard" },
//!       { "label": "Sales", "href": "/sales", "children": [ … ] } ] }
//! }
//! ```
//!
//! The theme is validated when the plugin is loaded, stored in the core
//! catalog, and copied into an organization's `ui_themes` when the plugin is
//! installed there. The web app then renders the layout and navigation the
//! organization's active theme names.

use std::path::PathBuf;

use serde_json::{Map, Value};
use thiserror::Error;

/// Layout used when a theme does not name one.
pub const DEFAULT_LAYOUT: &str = "default";

/// Error-page set used when a theme does not name one.
pub const DEFAULT_ERROR_PAGES: &str = "default";

const MAX_NAV_ITEMS: usize = 50;
const MAX_NAV_DEPTH: usize = 3;

#[derive(Debug, Error)]
pub enum ThemeError {
    #[error("filesystem error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{path}: not valid JSON: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("{path}: {reason}")]
    Invalid { path: PathBuf, reason: String },
}

/// A parsed theme.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeDocument {
    pub name: String,
    pub label: String,
    pub color_mode: String,
    /// `light`, `dark`, `radius` and `font_sans`.
    pub tokens: Value,
    /// Which layout component renders the app (`default`, `desk`, `custom`, …).
    pub layout: String,
    /// Navigation for the layout's navbar, if the theme provides one.
    pub nav: Option<Value>,
    /// Which set of error pages (404, 403, error cards, …) the app shows.
    pub error_pages: String,
    /// Plugin-defined layout body when `layout` is not a built-in name.
    pub layout_def: Option<Value>,
    /// Plugin-defined error-set body when `error_pages` is not a built-in name.
    pub error_pages_def: Option<Value>,
    /// Shell settings beyond colours: `home` (where `/` and the brand link lead) and
    /// `branding` (product name, logo, favicon). See [`validate_shell`].
    pub shell: Option<Value>,
}

/// Parse a theme's JSON file. `path` is only used in error messages.
/// `fallback_name` / `fallback_label` come from `[theme]` in `plugin.toml` and
/// are used when the JSON does not repeat them.
pub fn parse_theme(
    text: &str,
    path: &std::path::Path,
    fallback_name: &str,
    fallback_label: Option<&str>,
) -> Result<ThemeDocument, ThemeError> {
    let invalid = |reason: &str| ThemeError::Invalid {
        path: path.to_path_buf(),
        reason: reason.to_string(),
    };
    let value: Value = serde_json::from_str(text).map_err(|source| ThemeError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    let Value::Object(mut object) = value else {
        return Err(invalid("the theme file must be a JSON object"));
    };

    let text_field = |object: &Map<String, Value>, key: &str| -> Option<String> {
        object.get(key).and_then(Value::as_str).map(str::to_string)
    };
    let name = text_field(&object, "name").unwrap_or_else(|| fallback_name.to_string());
    let label = text_field(&object, "label")
        .or_else(|| fallback_label.map(str::to_string))
        .unwrap_or_else(|| name.clone());
    let color_mode = text_field(&object, "color_mode").unwrap_or_else(|| "system".to_string());
    if !["light", "dark", "system"].contains(&color_mode.as_str()) {
        return Err(invalid(
            "`color_mode` must be \"light\", \"dark\" or \"system\"",
        ));
    }

    let layout = text_field(&object, "layout").unwrap_or_else(|| DEFAULT_LAYOUT.to_string());
    if !is_identifier(&layout) {
        return Err(invalid(
            "`layout` must be lowercase letters, digits, `-` and `_`, starting with a letter",
        ));
    }

    let error_pages =
        text_field(&object, "error_pages").unwrap_or_else(|| DEFAULT_ERROR_PAGES.to_string());
    if !is_identifier(&error_pages) {
        return Err(invalid(
            "`error_pages` must be lowercase letters, digits, `-` and `_`, starting with a letter",
        ));
    }

    let nav = match object.remove("nav") {
        None | Some(Value::Null) => None,
        Some(nav) => {
            validate_nav(&nav).map_err(|reason| invalid(&format!("`nav`: {reason}")))?;
            Some(nav)
        }
    };

    for required in ["light", "dark"] {
        let Some(palette) = object.get(required).and_then(Value::as_object) else {
            return Err(invalid(&format!(
                "`{required}` must be an object of colour tokens"
            )));
        };
        validate_palette(palette).map_err(|reason| invalid(&format!("`{required}`: {reason}")))?;
    }
    let mut tokens = serde_json::json!({
        "light": object.remove("light").unwrap_or(Value::Null),
        "dark": object.remove("dark").unwrap_or(Value::Null),
        "radius": text_field(&object, "radius").unwrap_or_else(|| "0.375rem".to_string()),
        "font_sans": text_field(&object, "font_sans").unwrap_or_default(),
    });
    // Optional tokens that are not colours. Every value ends up inside a CSS rule, so
    // each is checked like a colour value.
    for key in ["radius", "font_sans", "font_mono", "font_heading"] {
        if let Some(value) = object.get(key) {
            let value = value
                .as_str()
                .ok_or_else(|| invalid(&format!("`{key}` must be a string")))?;
            if !is_safe_token_value(value) {
                return Err(invalid(&format!("`{key}`: {UNSAFE_VALUE}")));
            }
            tokens[key] = Value::String(value.to_string());
        }
    }
    if let Some(density) = object.get("density") {
        match density.as_str() {
            Some(value @ ("compact" | "comfortable" | "spacious")) => {
                tokens["density"] = Value::String(value.to_string());
            }
            _ => {
                return Err(invalid(
                    "`density` must be \"compact\", \"comfortable\" or \"spacious\"",
                ));
            }
        }
    }

    if let Some(shape) = object.get("shape")
        && !shape.is_null()
    {
        validate_shape(shape).map_err(|reason| invalid(&format!("`shape`: {reason}")))?;
        tokens["shape"] = shape.clone();
    }

    if let Some(type_) = object.get("type")
        && !type_.is_null()
    {
        validate_type(type_).map_err(|reason| invalid(&format!("`type`: {reason}")))?;
        tokens["type"] = type_.clone();
    }

    for (key, check) in [
        (
            "elevation",
            super::theme_extras::validate_elevation as fn(&Value) -> Result<(), String>,
        ),
        ("motion", super::theme_extras::validate_motion),
        ("focus", super::theme_extras::validate_focus),
        ("apps", super::theme_extras::validate_apps),
        ("launcher", super::theme_extras::validate_launcher),
        ("field", super::theme_extras::validate_field),
        ("table", super::theme_extras::validate_table),
    ] {
        if let Some(value) = object.get(key)
            && !value.is_null()
        {
            check(value).map_err(|reason| invalid(&format!("`{key}`: {reason}")))?;
            tokens[key] = value.clone();
        }
    }

    let mut shell = Map::new();
    // `css` names a file of the plugin; the catalog reads and checks it (`theme_css`) and
    // replaces this with the checked text under `shell.css`.
    if let Some(css) = object.remove("css")
        && !css.is_null()
    {
        let file = css
            .as_str()
            .filter(|file| is_plugin_file(file, ".css"))
            .ok_or_else(|| {
                invalid(
                    "`css` must be the path of a .css file in the plugin (no `..`, no leading `/`)",
                )
            })?;
        shell.insert("css_file".into(), Value::String(file.to_string()));
    }
    // `icons` names an SVG sprite; the catalog reads and checks it (`theme_icons`) and
    // replaces this with the checked icons under `shell.icons`.
    if let Some(icons) = object.remove("icons")
        && !icons.is_null()
    {
        let file = icons
            .as_str()
            .filter(|file| is_plugin_file(file, ".svg"))
            .ok_or_else(|| {
                invalid(
                    "`icons` must be the path of a .svg file in the plugin (no `..`, no leading `/`)",
                )
            })?;
        shell.insert("icons_file".into(), Value::String(file.to_string()));
    }
    for key in ["home", "branding"] {
        if let Some(value) = object.remove(key)
            && !value.is_null()
        {
            shell.insert(key.to_string(), value);
        }
    }
    let shell = if shell.is_empty() {
        None
    } else {
        let shell = Value::Object(shell);
        validate_shell(&shell).map_err(|reason| invalid(&reason))?;
        Some(shell)
    };

    Ok(ThemeDocument {
        name,
        label,
        color_mode,
        tokens,
        layout,
        nav,
        error_pages,
        layout_def: None,
        error_pages_def: None,
        shell,
    })
}

const MAX_TOKENS: usize = 120;
const UNSAFE_VALUE: &str =
    "values must be at most 200 characters and cannot contain `;`, `{`, `}`, `<`, `>` or `\\`";

/// Colour tokens: `--name: value` pairs written into the app's stylesheet. Names are
/// lowercase words joined by `-`; values cannot end the declaration or the rule.
fn validate_palette(palette: &Map<String, Value>) -> Result<(), String> {
    if palette.len() > MAX_TOKENS {
        return Err(format!("more than {MAX_TOKENS} tokens"));
    }
    for (name, value) in palette {
        let valid_name = name.len() <= 48
            && name.starts_with(|c: char| c.is_ascii_lowercase())
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !valid_name {
            return Err(format!(
                "`{name}` is not a token name (lowercase letters, digits, `-`)"
            ));
        }
        let value = value
            .as_str()
            .ok_or_else(|| format!("`{name}` must be a string"))?;
        if !is_safe_token_value(value) {
            return Err(format!("`{name}`: {UNSAFE_VALUE}"));
        }
    }
    Ok(())
}

/// Steps of the corner-radius scale (`rounded-sm`, `rounded-lg`, …).
pub const SHAPE_STEPS: &[&str] = &["xs", "sm", "md", "lg", "xl", "2xl", "3xl", "4xl"];

/// What the app rounds by role (`rounded-button`, `rounded-card`, …).
pub const SHAPE_ROLES: &[&str] = &[
    "button", "field", "card", "chip", "menu", "dialog", "tooltip", "tile", "avatar",
];

/// `shape` is `{ "scale"?: { <step>: <length> }, "roles"?: { <role>: <corner> } }`, where a
/// corner value is a step, `none`, `full` or a length, or up to four of them separated by spaces. Lengths are `0` or a number with `px`, `rem`, `em` or `%`.
fn validate_shape(shape: &Value) -> Result<(), String> {
    let shape = shape.as_object().ok_or("must be an object")?;
    for key in shape.keys() {
        if key != "scale" && key != "roles" {
            return Err(format!("unknown key `{key}`; use `scale` and `roles`"));
        }
    }
    if let Some(scale) = shape.get("scale") {
        let scale = scale.as_object().ok_or("`scale` must be an object")?;
        for (step, value) in scale {
            if !SHAPE_STEPS.contains(&step.as_str()) {
                return Err(format!(
                    "unknown step `{step}`; steps are {}",
                    SHAPE_STEPS.join(", ")
                ));
            }
            let value = value.as_str().unwrap_or_default();
            if !is_shape_length(value) {
                return Err(format!(
                    "`scale.{step}` must be a length such as \"0.5rem\" or \"8px\""
                ));
            }
        }
    }
    if let Some(roles) = shape.get("roles") {
        let roles = roles.as_object().ok_or("`roles` must be an object")?;
        for (role, value) in roles {
            if !SHAPE_ROLES.contains(&role.as_str()) {
                return Err(format!(
                    "unknown role `{role}`; roles are {}",
                    SHAPE_ROLES.join(", ")
                ));
            }
            // One value for all corners, or up to four in CSS order (top-left, top-right,
            // bottom-right, bottom-left): Material's field is `"xs xs none none"`.
            let value = value.as_str().unwrap_or_default();
            let corners: Vec<&str> = value.split_whitespace().collect();
            let ok = (1..=4).contains(&corners.len())
                && corners.iter().all(|corner| {
                    SHAPE_STEPS.contains(corner)
                        || *corner == "none"
                        || *corner == "full"
                        || is_shape_length(corner)
                });
            if !ok {
                return Err(format!(
                    "`roles.{role}` must be a step ({}), `none`, `full` or a length, or up to four of them for the corners",
                    SHAPE_STEPS.join(", ")
                ));
            }
        }
    }
    Ok(())
}

/// Steps of the font-size scale (`text-sm`, `text-lg`, …).
pub const TYPE_STEPS: &[&str] = &["xs", "sm", "base", "lg", "xl", "2xl", "3xl", "4xl", "5xl"];

/// What the app sets text in, by role (`type-title`, `type-label`, …).
pub const TYPE_ROLES: &[&str] = &[
    "display",
    "headline",
    "title",
    "heading",
    "subheading",
    "body",
    "label",
    "caption",
    "metric",
    "button",
    "column",
];

/// The properties of a type role.
pub const TYPE_PROPERTIES: &[&str] = &[
    "size",
    "line_height",
    "weight",
    "tracking",
    "family",
    "case",
];

/// `type` is `{ "scale"?: { <step>: <length> }, "roles"?: { <role>: { "size"?, "line_height"?,
/// "weight"?, "tracking"?, "family"?, "case"? } } }`.
fn validate_type(type_: &Value) -> Result<(), String> {
    let type_ = type_.as_object().ok_or("must be an object")?;
    for key in type_.keys() {
        if key != "scale" && key != "roles" {
            return Err(format!("unknown key `{key}`; use `scale` and `roles`"));
        }
    }
    if let Some(scale) = type_.get("scale") {
        let scale = scale.as_object().ok_or("`scale` must be an object")?;
        for (step, value) in scale {
            if !TYPE_STEPS.contains(&step.as_str()) {
                return Err(format!(
                    "unknown step `{step}`; steps are {}",
                    TYPE_STEPS.join(", ")
                ));
            }
            if !is_font_size(value.as_str().unwrap_or_default()) {
                return Err(format!(
                    "`scale.{step}` must be a length such as \"0.875rem\" or \"14px\""
                ));
            }
        }
    }
    if let Some(roles) = type_.get("roles") {
        let roles = roles.as_object().ok_or("`roles` must be an object")?;
        for (role, spec) in roles {
            if !TYPE_ROLES.contains(&role.as_str()) {
                return Err(format!(
                    "unknown role `{role}`; roles are {}",
                    TYPE_ROLES.join(", ")
                ));
            }
            let spec = spec
                .as_object()
                .ok_or_else(|| format!("`roles.{role}` must be an object"))?;
            for (property, value) in spec {
                validate_type_property(property, value)
                    .map_err(|reason| format!("`roles.{role}.{property}` {reason}"))?;
            }
        }
    }
    Ok(())
}

fn validate_type_property(property: &str, value: &Value) -> Result<(), String> {
    let text = value.as_str().unwrap_or_default();
    let ok = match property {
        "size" => TYPE_STEPS.contains(&text) || is_font_size(text),
        "line_height" => is_line_height(value),
        "weight" => is_font_weight(value),
        "tracking" => text == "normal" || is_signed_length(text, &["em", "px", "rem"]),
        "family" => ["sans", "heading", "mono"].contains(&text),
        "case" => ["none", "upper", "lower", "capitalize"].contains(&text),
        _ => {
            return Err(format!(
                "is unknown; properties are {}",
                TYPE_PROPERTIES.join(", ")
            ));
        }
    };
    if ok {
        return Ok(());
    }
    Err(match property {
        "size" => format!("must be a step ({}) or a length", TYPE_STEPS.join(", ")),
        "line_height" => "must be a number such as 1.5 or a length such as \"1.25rem\"".into(),
        "weight" => "must be 100, 200, … 900".into(),
        "tracking" => "must be `normal` or a length in em, px or rem".into(),
        "family" => "must be `sans`, `heading` or `mono`".into(),
        _ => "must be `none`, `upper`, `lower` or `capitalize`".into(),
    })
}

/// A positive length in `rem`, `px` or `em`.
fn is_font_size(value: &str) -> bool {
    value != "0" && is_signed_length(value, &["rem", "px", "em"]) && !value.starts_with('-')
}

/// A unitless number up to 4, or a positive length in `rem`, `px` or `em`.
fn is_line_height(value: &Value) -> bool {
    if let Some(n) = value.as_f64() {
        return n > 0.0 && n <= 4.0;
    }
    let text = value.as_str().unwrap_or_default();
    if text.chars().all(|c| c.is_ascii_digit() || c == '.') && text.len() <= 6 {
        return text.parse::<f64>().is_ok_and(|n| n > 0.0 && n <= 4.0);
    }
    is_font_size(text)
}

/// 100 to 900 in steps of 100, as a number or a string.
fn is_font_weight(value: &Value) -> bool {
    let n = match value {
        Value::Number(n) => n.as_u64(),
        Value::String(s) if s.len() == 3 => s.parse::<u64>().ok(),
        _ => None,
    };
    n.is_some_and(|n| (100..=900).contains(&n) && n % 100 == 0)
}

/// `0`, or a number (negative allowed) followed by one of `units`.
fn is_signed_length(value: &str, units: &[&str]) -> bool {
    if value == "0" {
        return true;
    }
    // Try every unit: `rem` also ends with `em`.
    units.iter().any(|unit| {
        let Some(number) = value.strip_suffix(unit) else {
            return false;
        };
        let digits = number.strip_prefix('-').unwrap_or(number);
        !digits.is_empty()
            && digits.len() <= 10
            && digits.chars().all(|c| c.is_ascii_digit() || c == '.')
            && digits.parse::<f64>().is_ok_and(f64::is_finite)
    })
}

/// `0`, or a non-negative number followed by `px`, `rem`, `em` or `%`.
fn is_shape_length(value: &str) -> bool {
    if value == "0" {
        return true;
    }
    let Some(unit) = ["rem", "px", "em", "%"]
        .into_iter()
        .find(|unit| value.ends_with(unit))
    else {
        return false;
    };
    let number = &value[..value.len() - unit.len()];
    number.len() <= 10
        && !number.starts_with('-')
        && number
            .parse::<f64>()
            .is_ok_and(|n| n.is_finite() && n >= 0.0)
        && number.chars().all(|c| c.is_ascii_digit() || c == '.')
}

/// A CSS value that stays inside its declaration.
pub(crate) fn is_safe_token_value(value: &str) -> bool {
    value.chars().count() <= 200
        && !value
            .chars()
            .any(|c| matches!(c, ';' | '{' | '}' | '<' | '>' | '\\') || c.is_control())
}

/// A relative path to a file with `extension` (`.css`, `.svg`) inside the plugin.
fn is_plugin_file(path: &str, extension: &str) -> bool {
    path.len() <= 200
        && path.ends_with(extension)
        && !path.starts_with('/')
        && path.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        })
}

/// `shell` is `{ "home"?: { "route": "/sales" }, "branding"?: { "product_name"?,
/// "logo"?, "favicon"? } }`. Logos are `assets/…` files of the theme plugin or https URLs.
pub fn validate_shell(shell: &Value) -> Result<(), String> {
    let shell = shell.as_object().ok_or("`shell` must be an object")?;
    // `css_file` and `icons_file` are paths until the catalog replaces them with the
    // checked `css` text and `icons` map.
    for key in shell.keys() {
        if !["home", "branding", "css_file", "css", "icons_file", "icons", "screens"].contains(&key.as_str()) {
            return Err(format!("`shell`: unknown key `{key}`"));
        }
    }
    if let Some(home) = shell.get("home") {
        let home = home.as_object().ok_or("`home` must be an object")?;
        for key in home.keys() {
            if key != "route" {
                return Err(format!("`home`: unknown key `{key}`"));
            }
        }
        let route = home
            .get("route")
            .and_then(Value::as_str)
            .ok_or("`home.route` is required")?;
        if !(route.starts_with('/') && is_safe_href(route)) || route.len() > 200 {
            return Err("`home.route` must be an app path starting with `/`".into());
        }
    }
    if let Some(branding) = shell.get("branding") {
        let branding = branding.as_object().ok_or("`branding` must be an object")?;
        for (key, value) in branding {
            let text = value
                .as_str()
                .ok_or_else(|| format!("`branding.{key}` must be a string"))?;
            match key.as_str() {
                "product_name" => {
                    if text.trim().is_empty() || text.chars().count() > 60 {
                        return Err("`branding.product_name` must be 1 to 60 characters".into());
                    }
                }
                "logo" | "favicon" => {
                    if !is_asset_reference(text) {
                        return Err(format!(
                            "`branding.{key}` must be a file under `assets/` or an https URL"
                        ));
                    }
                }
                _ => return Err(format!("`branding`: unknown key `{key}`")),
            }
        }
    }
    Ok(())
}

/// `assets/brand/logo.svg` (a file the plugin ships) or an `https://` URL.
pub fn is_asset_reference(value: &str) -> bool {
    if value.len() > 300 {
        return false;
    }
    if let Some(rest) = value.strip_prefix("https://") {
        return !rest.is_empty() && !value.contains(['"', '\'', '<', '>', ' ', '\\']);
    }
    value.strip_prefix("assets/").is_some_and(|rest| {
        !rest.is_empty()
            && rest.split('/').all(|part| {
                !part.is_empty()
                    && part != "."
                    && part != ".."
                    && part
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            })
    })
}

/// Layout and page-layout names: lowercase letters, digits, `-` and `_`,
/// starting with a letter.
pub fn is_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    value.len() <= 32
        && characters
            .next()
            .is_some_and(|first| first.is_ascii_lowercase())
        && characters.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// `nav` is `{ "header"?: string, "footer"?: [items], "items": [ { label, href, icon?, description?, children? } ] }`.
fn validate_nav(nav: &Value) -> Result<(), String> {
    let object = nav.as_object().ok_or("must be an object")?;
    if let Some(header) = object.get("header")
        && !header.is_string()
    {
        return Err("`header` must be a string".into());
    }
    let items = object
        .get("items")
        .and_then(Value::as_array)
        .ok_or("`items` must be an array")?;
    let mut count = 0;
    validate_items(items, 1, &mut count)?;
    // `footer`: the links a `nav` block with `source: "footer"` draws (footer link columns).
    match object.get("footer") {
        None | Some(Value::Null) => Ok(()),
        Some(Value::Array(items)) => {
            let mut count = 0;
            validate_items(items, 1, &mut count).map_err(|reason| format!("`footer`: {reason}"))
        }
        Some(_) => Err("`footer` must be an array of items".into()),
    }
}

pub(super) fn validate_items(items: &[Value], depth: usize, count: &mut usize) -> Result<(), String> {
    if depth > MAX_NAV_DEPTH {
        return Err(format!(
            "items are nested more than {MAX_NAV_DEPTH} levels deep"
        ));
    }
    for item in items {
        *count += 1;
        if *count > MAX_NAV_ITEMS {
            return Err(format!("more than {MAX_NAV_ITEMS} items"));
        }
        let object = item.as_object().ok_or("each item must be an object")?;
        let label = object
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if label.trim().is_empty() {
            return Err("each item needs a non-empty `label`".into());
        }
        // An item is a link, a group of links, or both.
        let href = object.get("href");
        match href {
            None => {}
            Some(Value::String(href)) if is_safe_href(href) => {}
            Some(_) => {
                return Err(format!(
                    "`href` of `{label}` must be an app path starting with `/` or an http(s) URL"
                ));
            }
        }
        if let Some(icon) = object.get("icon")
            && !icon.is_string()
        {
            return Err(format!("`icon` of `{label}` must be a string"));
        }
        match object.get("badge") {
            None | Some(Value::Null) => {}
            // A short fixed label: `New`, `Beta`.
            Some(Value::String(text))
                if !text.trim().is_empty()
                    && text.chars().count() <= 12
                    && is_safe_token_value(text) => {}
            Some(Value::Object(badge))
                if badge.len() == 1
                    && badge
                        .get("function")
                        .and_then(Value::as_str)
                        .is_some_and(|f| f.contains('.') && super::pages::is_function_ref(f)) => {}
            Some(_) => {
                return Err(format!(
                    "`badge` of `{label}` must be text of at most 12 characters or {{ \"function\": \"<plugin>.<function>\" }}"
                ));
            }
        }
        if let Some(description) = object.get("description") {
            let ok = description
                .as_str()
                .is_some_and(|text| text.chars().count() <= 200);
            if !ok {
                return Err(format!(
                    "`description` of `{label}` must be text of at most 200 characters"
                ));
            }
        }
        match object.get("children") {
            None => {
                if href.is_none() {
                    return Err(format!("`{label}` needs an `href` or `children`"));
                }
            }
            Some(Value::Array(children)) => validate_items(children, depth + 1, count)?,
            Some(_) => return Err(format!("`children` of `{label}` must be an array")),
        }
    }
    Ok(())
}

/// App-relative paths and http(s) URLs only: no `javascript:` or `data:`
/// links, and no protocol-relative `//host`.
pub(crate) fn is_safe_href(href: &str) -> bool {
    (href.starts_with('/') && !href.starts_with("//") && !href.contains('\\'))
        || href.starts_with("https://")
        || href.starts_with("http://")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn parse(json: &str) -> Result<ThemeDocument, ThemeError> {
        parse_theme(json, Path::new("theme.json"), "fallback", Some("Fallback"))
    }

    const COLOURS: &str = r##""light": {"background": "#fff"}, "dark": {"background": "#000"}"##;

    #[test]
    fn reads_tokens_layout_and_nav() -> Result<(), ThemeError> {
        let theme = parse(&format!(
            r#"{{ "name": "ocean", "label": "Ocean", "color_mode": "dark", "radius": "1rem",
                 "layout": "custom", "error_pages": "custom", {COLOURS},
                 "nav": {{ "header": "Ocean", "items": [
                    {{ "label": "Apps", "href": "/apps", "icon": "grid", "badge": "New" }},
                    {{ "label": "Leads", "href": "/crm/leads", "badge": {{ "function": "crm.open_leads" }} }},
                    {{ "label": "Sales", "children": [ {{ "label": "Orders", "href": "/sales/orders" }} ] }}
                 ] }} }}"#
        ))?;
        assert_eq!(theme.name, "ocean");
        assert_eq!(theme.layout, "custom");
        assert_eq!(theme.error_pages, "custom");
        assert_eq!(theme.color_mode, "dark");
        assert_eq!(theme.tokens["radius"], "1rem");
        assert_eq!(theme.tokens["light"]["background"], "#fff");
        assert_eq!(
            theme.nav.as_ref().map(|n| n["header"].clone()),
            Some(Value::from("Ocean"))
        );
        Ok(())
    }

    #[test]
    fn defaults_come_from_the_manifest_and_the_default_layout() -> Result<(), ThemeError> {
        let theme = parse(&format!("{{ {COLOURS} }}"))?;
        assert_eq!(
            (theme.name.as_str(), theme.label.as_str()),
            ("fallback", "Fallback")
        );
        assert_eq!(theme.layout, DEFAULT_LAYOUT);
        assert_eq!(theme.error_pages, DEFAULT_ERROR_PAGES);
        assert_eq!(theme.color_mode, "system");
        assert!(theme.nav.is_none());
        Ok(())
    }

    #[test]
    fn rejects_malformed_themes() {
        let bad = [
            "[]".to_string(),
            "not json".to_string(),
            format!(r#"{{ "color_mode": "blue", {COLOURS} }}"#),
            format!(r#"{{ "layout": "Has Space", {COLOURS} }}"#),
            format!(r#"{{ "error_pages": "Has Space", {COLOURS} }}"#),
            r#"{ "light": {}, "dark": 3 }"#.to_string(),
            format!(r#"{{ "nav": {{ "items": [{{ "label": "x" }}] }}, {COLOURS} }}"#),
            format!(r#"{{ "nav": {{ "items": [{{ "label": "", "href": "/a" }}] }}, {COLOURS} }}"#),
            format!(
                r#"{{ "nav": {{ "items": [{{ "label": "x", "href": "javascript:alert(1)" }}] }}, {COLOURS} }}"#
            ),
            format!(
                r#"{{ "nav": {{ "items": [{{ "label": "x", "href": "//evil.example" }}] }}, {COLOURS} }}"#
            ),
            format!(r#"{{ "nav": {{ "items": "nope" }}, {COLOURS} }}"#),
        ];
        for json in bad {
            assert!(parse(&json).is_err(), "{json}");
        }
    }

    #[test]
    fn reads_v2_tokens_and_shell() -> Result<(), ThemeError> {
        let theme = parse(&format!(
            r#"{{ {COLOURS}, "font_mono": "JetBrains Mono, monospace", "density": "compact",
                 "home": {{ "route": "/sales/dashboard" }},
                 "branding": {{ "product_name": "Sea ERP", "logo": "assets/brand/logo.svg",
                                "favicon": "https://cdn.example/fav.ico" }} }}"#
        ))?;
        assert_eq!(theme.tokens["font_mono"], "JetBrains Mono, monospace");
        assert_eq!(theme.tokens["density"], "compact");
        let shell = theme.shell.unwrap_or_default();
        assert_eq!(shell["home"]["route"], "/sales/dashboard");
        assert_eq!(shell["branding"]["logo"], "assets/brand/logo.svg");
        Ok(())
    }

    #[test]
    fn reads_and_checks_the_shape() -> Result<(), ThemeError> {
        let theme = parse(
            r##"{ "light": {}, "dark": {}, "shape": {
                   "scale": { "sm": "0.5rem", "lg": "16px", "2xl": "0" },
                   "roles": { "button": "full", "field": "xs xs none none", "card": "1rem", "dialog": "none" } } }"##,
        )?;
        assert_eq!(theme.tokens["shape"]["roles"]["button"], "full");
        assert_eq!(theme.tokens["shape"]["scale"]["lg"], "16px");

        for bad in [
            r##"{ "light": {}, "dark": {}, "shape": "round" }"##,
            r##"{ "light": {}, "dark": {}, "shape": { "corners": {} } }"##,
            r##"{ "light": {}, "dark": {}, "shape": { "scale": { "huge": "1rem" } } }"##,
            r##"{ "light": {}, "dark": {}, "shape": { "scale": { "sm": "calc(1px)" } } }"##,
            r##"{ "light": {}, "dark": {}, "shape": { "scale": { "sm": "-1px" } } }"##,
            r##"{ "light": {}, "dark": {}, "shape": { "scale": { "sm": "1vw" } } }"##,
            r##"{ "light": {}, "dark": {}, "shape": { "scale": { "sm": 4 } } }"##,
            r##"{ "light": {}, "dark": {}, "shape": { "roles": { "navbar": "sm" } } }"##,
            r##"{ "light": {}, "dark": {}, "shape": { "roles": { "button": "pill" } } }"##,
            r##"{ "light": {}, "dark": {}, "shape": { "roles": { "button": "1px; color: red" } } }"##,
            r##"{ "light": {}, "dark": {}, "shape": { "roles": { "field": "xs xs xs xs xs" } } }"##,
            r##"{ "light": {}, "dark": {}, "shape": { "roles": { "field": "xs pill" } } }"##,
        ] {
            assert!(parse(bad).is_err(), "accepted: {bad}");
        }
        Ok(())
    }

    #[test]
    fn reads_and_checks_the_type() -> Result<(), ThemeError> {
        let theme = parse(
            r##"{ "light": {}, "dark": {}, "type": {
                   "scale": { "sm": "0.875rem", "4xl": "48px" },
                   "roles": {
                     "display": { "size": "4xl", "line_height": 1.1, "weight": 700, "tracking": "-0.02em" },
                     "label": { "size": "0.75rem", "line_height": "1rem", "weight": "500", "case": "upper" },
                     "headline": { "family": "heading", "tracking": "normal" },
                     "caption": { "tracking": "0.0625rem" } } } }"##,
        )?;
        assert_eq!(theme.tokens["type"]["roles"]["display"]["weight"], 700);
        assert_eq!(theme.tokens["type"]["scale"]["4xl"], "48px");

        for bad in [
            r##"{ "light": {}, "dark": {}, "type": "serif" }"##,
            r##"{ "light": {}, "dark": {}, "type": { "fonts": {} } }"##,
            r##"{ "light": {}, "dark": {}, "type": { "scale": { "huge": "1rem" } } }"##,
            r##"{ "light": {}, "dark": {}, "type": { "scale": { "sm": "-1rem" } } }"##,
            r##"{ "light": {}, "dark": {}, "type": { "scale": { "sm": "1vw" } } }"##,
            r##"{ "light": {}, "dark": {}, "type": { "roles": { "hero": {} } } }"##,
            r##"{ "light": {}, "dark": {}, "type": { "roles": { "title": "lg" } } }"##,
            r##"{ "light": {}, "dark": {}, "type": { "roles": { "title": { "color": "red" } } } }"##,
            r##"{ "light": {}, "dark": {}, "type": { "roles": { "title": { "size": "giant" } } } }"##,
            r##"{ "light": {}, "dark": {}, "type": { "roles": { "title": { "weight": 450 } } } }"##,
            r##"{ "light": {}, "dark": {}, "type": { "roles": { "title": { "weight": "bold" } } } }"##,
            r##"{ "light": {}, "dark": {}, "type": { "roles": { "title": { "line_height": 9 } } } }"##,
            r##"{ "light": {}, "dark": {}, "type": { "roles": { "title": { "tracking": "1px; color: red" } } } }"##,
            r##"{ "light": {}, "dark": {}, "type": { "roles": { "title": { "family": "Comic Sans" } } } }"##,
            r##"{ "light": {}, "dark": {}, "type": { "roles": { "title": { "case": "small-caps" } } } }"##,
        ] {
            assert!(parse(bad).is_err(), "accepted: {bad}");
        }
        Ok(())
    }

    #[test]
    fn rejects_values_that_escape_css() {
        let bad = [
            r##"{ "light": {"background": "red; } body { display: none"}, "dark": {} }"##,
            r##"{ "light": {"Background": "#fff"}, "dark": {} }"##,
            r##"{ "light": {"x": 3}, "dark": {} }"##,
            r##"{ "light": {}, "dark": {}, "font_sans": "a</style><script>" }"##,
            r##"{ "light": {}, "dark": {}, "density": "huge" }"##,
            r##"{ "light": {}, "dark": {}, "home": { "route": "https://evil.example" } }"##,
            r##"{ "light": {}, "dark": {}, "home": { "page": "x" } }"##,
            r##"{ "light": {}, "dark": {}, "branding": { "logo": "../secrets.txt" } }"##,
            r##"{ "light": {}, "dark": {}, "branding": { "logo": "assets/../x.svg" } }"##,
            r##"{ "light": {}, "dark": {}, "branding": { "logo": "http://insecure/x.svg" } }"##,
            r##"{ "light": {}, "dark": {}, "branding": { "colour": "red" } }"##,
            r##"{ "light": {}, "dark": {}, "nav": { "items": [ { "label": "A", "href": "/a", "description": 3 } ] } }"##,
            r##"{ "light": {}, "dark": {}, "nav": { "items": [ { "label": "A", "href": "/a", "badge": "far too long a badge" } ] } }"##,
            r##"{ "light": {}, "dark": {}, "nav": { "items": [ { "label": "A", "href": "/a", "badge": { "function": "count" } } ] } }"##,
            r##"{ "light": {}, "dark": {}, "nav": { "items": [ { "label": "A", "href": "/a", "badge": 3 } ] } }"##,
            r##"{ "light": {}, "dark": {}, "nav": { "items": [], "footer": "links" } }"##,
            r##"{ "light": {}, "dark": {}, "nav": { "items": [], "footer": [ { "label": "A" } ] } }"##,
        ];
        for json in bad {
            assert!(parse(json).is_err(), "{json}");
        }
    }

    #[test]
    fn the_built_in_default_is_a_valid_theme_manifest() -> Result<(), ThemeError> {
        let theme = parse_theme(
            aether_web::api::theme::DEFAULT_THEME_JSON,
            Path::new("web/src/lib/theme/default-theme.json"),
            "default",
            None,
        )?;
        assert_eq!(theme.name, "default");
        assert_eq!(theme.tokens["density"], "comfortable");
        Ok(())
    }

    #[test]
    fn asset_references() {
        for good in [
            "assets/logo.svg",
            "assets/brand/logo-dark.png",
            "https://cdn.example/a.svg",
        ] {
            assert!(is_asset_reference(good), "{good}");
        }
        for bad in [
            "logo.svg",
            "assets/",
            "assets//x",
            "assets/./x",
            "/assets/x",
            "https://",
            "data:image/png;base64,x",
        ] {
            assert!(!is_asset_reference(bad), "{bad}");
        }
    }

    #[test]
    fn nav_limits_depth_and_size() {
        let deep = r#"{ "items": [{ "label": "a", "children": [{ "label": "b", "children": [{ "label": "c", "children": [{ "label": "d", "href": "/d" }] }] }] }] }"#;
        assert!(validate_nav(&serde_json::from_str(deep).unwrap_or(Value::Null)).is_err());

        let many: Vec<Value> = (0..51)
            .map(|i| serde_json::json!({ "label": format!("i{i}"), "href": "/x" }))
            .collect();
        assert!(validate_nav(&serde_json::json!({ "items": many })).is_err());
    }

    #[test]
    fn identifiers_are_lowercase_names() {
        for good in ["default", "custom", "my-layout", "bare_2"] {
            assert!(is_identifier(good), "{good}");
        }
        for bad in ["", "Default", "1x", "a b", "a/b", &"x".repeat(40)] {
            assert!(!is_identifier(bad), "{bad}");
        }
    }
}
