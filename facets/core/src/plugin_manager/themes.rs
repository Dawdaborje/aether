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

    let mut shell = Map::new();
    // `css` names a file of the plugin; the catalog reads and checks it (`theme_css`) and
    // replaces this with the checked text under `shell.css`.
    if let Some(css) = object.remove("css")
        && !css.is_null()
    {
        let file = css
            .as_str()
            .filter(|file| is_stylesheet_path(file))
            .ok_or_else(|| {
                invalid(
                    "`css` must be the path of a .css file in the plugin (no `..`, no leading `/`)",
                )
            })?;
        shell.insert("css_file".into(), Value::String(file.to_string()));
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

/// A CSS value that stays inside its declaration.
pub(crate) fn is_safe_token_value(value: &str) -> bool {
    value.chars().count() <= 200
        && !value
            .chars()
            .any(|c| matches!(c, ';' | '{' | '}' | '<' | '>' | '\\') || c.is_control())
}

/// A relative path to a `.css` file inside the plugin.
fn is_stylesheet_path(path: &str) -> bool {
    path.len() <= 200
        && path.ends_with(".css")
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
    // `css_file` is a path until the catalog replaces it with the checked `css` text.
    for key in shell.keys() {
        if !["home", "branding", "css_file", "css"].contains(&key.as_str()) {
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

/// `nav` is `{ "header"?: string, "items": [ { label, href, icon?, children? } ] }`.
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
    validate_items(items, 1, &mut count)
}

fn validate_items(items: &[Value], depth: usize, count: &mut usize) -> Result<(), String> {
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
                    {{ "label": "Apps", "href": "/apps", "icon": "grid" }},
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
