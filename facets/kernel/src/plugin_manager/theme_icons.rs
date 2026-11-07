//! The icon sprite a theme plugin may ship (`"icons": "icons.svg"` in its theme file).
//!
//! A sprite is an SVG file of `<symbol id="name">` elements, one per icon. The web app
//! draws a theme's symbol wherever it shows the icon of that name (`bell`, `folder`,
//! `chevron-down`, …) and its own icon for every name the sprite leaves out.
//!
//! The markup ends up inside the app's pages, so it is not passed through: it is parsed,
//! every element and attribute is checked against short lists of plain shapes and
//! presentation attributes, and each symbol is written out again from what passed. A
//! sprite that uses anything else (scripts, `<use>`, `<image>`, `<foreignObject>`,
//! `style`, event handlers, links, `url()` references, text) refuses the plugin.
//!
//! The result is a JSON object: `{ "<name>": { "view_box", "attrs", "body" } }`, where
//! `attrs` are the presentation attributes set on the `<symbol>` (for example
//! `fill="none" stroke="currentColor"`) and `body` is the symbol's checked inner markup.

use std::path::Path;

use serde_json::{Map, Value};

use super::xml_tree::{Node, compile_xml};

pub const MAX_BYTES: usize = 256 * 1024;
pub const MAX_ICONS: usize = 500;
const MAX_NAME: usize = 64;
const MAX_VALUE: usize = 8 * 1024;
const MAX_DEPTH: usize = 6;

/// Shapes a symbol may contain.
const SHAPES: &[&str] = &[
    "g", "path", "circle", "ellipse", "line", "polyline", "polygon", "rect",
];

/// Geometry attributes, by shape.
fn geometry(tag: &str) -> &'static [&'static str] {
    match tag {
        "path" => &["d"],
        "circle" => &["cx", "cy", "r"],
        "ellipse" => &["cx", "cy", "rx", "ry"],
        "line" => &["x1", "y1", "x2", "y2"],
        "polyline" | "polygon" => &["points"],
        "rect" => &["x", "y", "width", "height", "rx", "ry"],
        _ => &[],
    }
}

/// Presentation attributes allowed on `<symbol>` and on every shape.
const PRESENTATION: &[&str] = &[
    "fill",
    "fill-opacity",
    "fill-rule",
    "clip-rule",
    "stroke",
    "stroke-width",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-dasharray",
    "stroke-dashoffset",
    "stroke-opacity",
    "opacity",
    "vector-effect",
    "shape-rendering",
];

/// Check `svg` and return its icons, keyed by symbol id.
pub fn sanitize(svg: &str, source: &Path) -> Result<Value, String> {
    if svg.len() > MAX_BYTES {
        return Err(format!("is larger than {} KB", MAX_BYTES / 1024));
    }
    if svg.contains('\0') {
        return Err("contains a NUL character".into());
    }
    if svg.contains("<!ENTITY") {
        return Err("declares XML entities".into());
    }
    let root = compile_xml(svg, source).map_err(|error| error.to_string())?;
    if root.tag != "svg" {
        return Err(format!(
            "must have `<svg>` as its root, not `<{}>`",
            root.tag
        ));
    }
    let mut icons = Map::new();
    for child in &root.children {
        match child.tag.as_str() {
            "symbol" => add_symbol(child, &mut icons)?,
            "defs" => {
                for symbol in &child.children {
                    if symbol.tag != "symbol" {
                        return Err(format!(
                            "`<defs>` may only hold `<symbol>`, not `<{}>`",
                            symbol.tag
                        ));
                    }
                    add_symbol(symbol, &mut icons)?;
                }
            }
            "title" | "desc" => {}
            other => {
                return Err(format!(
                    "may only hold `<symbol>` elements (in `<svg>` or `<defs>`), not `<{other}>`"
                ));
            }
        }
    }
    if icons.is_empty() {
        return Err("has no `<symbol id=\"…\">` icons".into());
    }
    Ok(Value::Object(icons))
}

fn add_symbol(symbol: &Node, icons: &mut Map<String, Value>) -> Result<(), String> {
    let name = symbol
        .attribute("id")
        .ok_or("every `<symbol>` needs an `id`: the icon's name")?;
    if !is_icon_name(name) {
        return Err(format!(
            "`{name}` is not an icon name: use lowercase letters, digits and `-`, at most {MAX_NAME} characters"
        ));
    }
    if icons.contains_key(name) {
        return Err(format!("icon `{name}` is defined twice"));
    }
    if icons.len() >= MAX_ICONS {
        return Err(format!("has more than {MAX_ICONS} icons"));
    }
    let view_box = symbol
        .attribute("viewBox")
        .ok_or_else(|| format!("icon `{name}` needs a `viewBox`"))?;
    if !is_view_box(view_box) {
        return Err(format!(
            "icon `{name}`: `viewBox` must be four numbers, such as \"0 0 24 24\""
        ));
    }
    let mut attrs = Map::new();
    for (key, value) in &symbol.attributes {
        match key.as_str() {
            "id" | "viewBox" => {}
            key if PRESENTATION.contains(&key) => {
                check_value(name, key, value)?;
                attrs.insert(key.to_string(), Value::String(value.clone()));
            }
            other => return Err(format!("icon `{name}`: attribute `{other}` is not allowed")),
        }
    }
    if symbol.text.is_some() {
        return Err(format!("icon `{name}` contains text"));
    }
    let mut body = String::new();
    for child in &symbol.children {
        write_shape(name, child, 1, &mut body)?;
    }
    if body.is_empty() {
        return Err(format!("icon `{name}` is empty"));
    }
    let mut icon = Map::new();
    icon.insert("view_box".into(), Value::String(view_box.to_string()));
    icon.insert("attrs".into(), Value::Object(attrs));
    icon.insert("body".into(), Value::String(body));
    icons.insert(name.to_string(), Value::Object(icon));
    Ok(())
}

fn write_shape(icon: &str, node: &Node, depth: usize, out: &mut String) -> Result<(), String> {
    if matches!(node.tag.as_str(), "title" | "desc") {
        return Ok(());
    }
    if !SHAPES.contains(&node.tag.as_str()) {
        return Err(format!(
            "icon `{icon}`: `<{}>` is not allowed; use g, path, circle, ellipse, line, polyline, polygon or rect",
            node.tag
        ));
    }
    if depth > MAX_DEPTH {
        return Err(format!("icon `{icon}` nests groups too deeply"));
    }
    if node.text.is_some() {
        return Err(format!("icon `{icon}`: `<{}>` contains text", node.tag));
    }
    out.push('<');
    out.push_str(&node.tag);
    for (key, value) in &node.attributes {
        let allowed = geometry(&node.tag).contains(&key.as_str())
            || PRESENTATION.contains(&key.as_str())
            || key == "transform";
        if !allowed {
            return Err(format!(
                "icon `{icon}`: attribute `{key}` is not allowed on `<{}>`",
                node.tag
            ));
        }
        check_value(icon, key, value)?;
        out.push(' ');
        out.push_str(key);
        out.push_str("=\"");
        out.push_str(value);
        out.push('"');
    }
    if node.children.is_empty() {
        out.push_str("/>");
        return Ok(());
    }
    if node.tag != "g" {
        return Err(format!(
            "icon `{icon}`: only `<g>` may contain other shapes"
        ));
    }
    out.push('>');
    for child in &node.children {
        write_shape(icon, child, depth + 1, out)?;
    }
    out.push_str("</g>");
    Ok(())
}

/// Attribute values are numbers, path data, colour names or `#hex`, and keywords.
/// `transform` may also use parentheses. Nothing else gets through, so no value can
/// close the attribute, start markup, or reference anything (`url(…)`).
fn check_value(icon: &str, key: &str, value: &str) -> Result<(), String> {
    let parentheses = key == "transform";
    let ok = !value.is_empty()
        && value.len() <= MAX_VALUE
        && value.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, ' ' | '\n' | '\t' | '\r' | ',' | '.' | '-' | '+' | '#' | '%')
                || (parentheses && matches!(c, '(' | ')'))
        })
        // Only `transform` admits parentheses, so only it could spell `url(`.
        && !(parentheses && value.to_ascii_lowercase().contains("url"));
    if ok {
        Ok(())
    } else {
        Err(format!(
            "icon `{icon}`: the value of `{key}` is not allowed"
        ))
    }
}

pub fn is_icon_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn is_view_box(value: &str) -> bool {
    let parts: Vec<&str> = value
        .split(|c: char| c == ' ' || c == ',')
        .filter(|part| !part.is_empty())
        .collect();
    parts.len() == 4
        && parts
            .iter()
            .all(|part| part.parse::<f64>().is_ok_and(f64::is_finite))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(svg: &str) -> Result<Value, String> {
        sanitize(svg, Path::new("icons.svg"))
    }

    #[test]
    fn keeps_symbols_and_rewrites_their_markup() -> Result<(), String> {
        let icons = check(
            r##"<?xml version="1.0"?>
            <svg xmlns="http://www.w3.org/2000/svg">
              <title>Sea icons</title>
              <symbol id="bell" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                <path d="M6 16v-5a6 6 0 0 1 12 0v5l1.5 2h-15z"/>
                <g transform="translate(0 1)"><path d="M10 21h4"/></g>
              </symbol>
              <defs><symbol id="folder" viewBox="0 0 16 16"><rect x="1" y="3" width="14" height="10" rx="1"/></symbol></defs>
            </svg>"##,
        )?;
        let bell = &icons["bell"];
        assert_eq!(bell["view_box"], "0 0 24 24");
        assert_eq!(bell["attrs"]["stroke"], "currentColor");
        assert_eq!(
            bell["body"],
            r#"<path d="M6 16v-5a6 6 0 0 1 12 0v5l1.5 2h-15z"/><g transform="translate(0 1)"><path d="M10 21h4"/></g>"#
        );
        assert_eq!(
            icons["folder"]["body"],
            r#"<rect x="1" y="3" width="14" height="10" rx="1"/>"#
        );
        Ok(())
    }

    #[test]
    fn refuses_anything_but_plain_shapes() {
        let symbol = |inner: &str| {
            format!(r#"<svg><symbol id="x" viewBox="0 0 24 24">{inner}</symbol></svg>"#)
        };
        for bad in [
            symbol(r#"<script>alert(1)</script>"#),
            symbol(r##"<use href="#y"/>"##),
            symbol(r#"<image href="https://evil.example/x.png"/>"#),
            symbol(r#"<foreignObject><div/></foreignObject>"#),
            symbol(r#"<path d="M0 0" onclick="alert(1)"/>"#),
            symbol(r#"<path d="M0 0" style="fill:red"/>"#),
            symbol(r#"<path d="M0 0" fill="url(#g)"/>"#),
            symbol(r#"<path d="M0 0" fill="&quot;/&gt;&lt;script&gt;"/>"#),
            symbol(r#"<path d="M0 0">text</path>"#),
            symbol(r#"<path d="M0 0"><path d="M1 1"/></path>"#),
            symbol(""),
            r#"<svg><symbol id="Bad Name" viewBox="0 0 24 24"><path d="M0 0"/></symbol></svg>"#.into(),
            r#"<svg><symbol id="x"><path d="M0 0"/></symbol></svg>"#.into(),
            r#"<svg><symbol id="x" viewBox="0 0 24"><path d="M0 0"/></symbol></svg>"#.into(),
            r#"<svg><symbol id="x" viewBox="0 0 24 24" onload="x"><path d="M0 0"/></symbol></svg>"#.into(),
            r#"<svg><symbol id="x" viewBox="0 0 24 24"><path d="M0 0"/></symbol><symbol id="x" viewBox="0 0 24 24"><path d="M0 0"/></symbol></svg>"#.into(),
            r#"<svg><path d="M0 0"/></svg>"#.into(),
            r#"<svg/>"#.into(),
            r#"<html><symbol id="x" viewBox="0 0 24 24"><path d="M0 0"/></symbol></html>"#.into(),
            r#"<!DOCTYPE svg [<!ENTITY x "y">]><svg><symbol id="x" viewBox="0 0 24 24"><path d="M0 0"/></symbol></svg>"#.into(),
        ] {
            assert!(check(&bad).is_err(), "accepted: {bad}");
        }
    }

    #[test]
    fn limits_size() {
        let big = format!("<svg>{}</svg>", " ".repeat(MAX_BYTES));
        assert!(check(&big).is_err());
    }
}
