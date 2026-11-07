//! Sign-in screens a theme plugin ships as XML under `screens/<name>.xml`.
//!
//! ```xml
//! <screen name="login" layout="split">
//!   <aside tone="primary" side="left">
//!     <brand/>
//!     <heading level="2">Welcome back</heading>
//!     <text>Pick up where you left off.</text>
//!     <image src="assets/brand/hero.svg" alt=""/>
//!   </aside>
//!   <main>
//!     <heading level="1">Sign in</heading>
//!     <login-form/>
//!     <oauth-buttons/>
//!   </main>
//! </screen>
//! ```
//!
//! The server keeps the screen as JSON in the theme's `shell.screens.<name>`; the web app
//! draws it, and the forms keep working as they do on the built-in screens.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use super::shell_defs::{ShellDefError, list_files};
use super::themes::{is_asset_reference, is_safe_href};
use super::xml_tree::{Node, compile_xml};

pub const SCREENS_DIR: &str = "screens";

/// The screens a theme may replace.
pub const SCREEN_NAMES: &[&str] = &["login", "register", "forgot-password", "reset-password"];
const LAYOUTS: &[&str] = &["split", "center", "stack"];
const TONES: &[&str] = &["default", "muted", "primary"];
const MAX_BLOCKS: usize = 20;
const MAX_TEXT: usize = 400;

#[derive(Debug, Clone, PartialEq)]
pub struct ScreenDef {
    pub name: String,
    pub source: PathBuf,
    pub definition: Value,
}

pub async fn discover_screens(package_dir: &Path) -> Result<Vec<ScreenDef>, ShellDefError> {
    let files = list_files(package_dir, SCREENS_DIR, &["xml"]).await?;
    let mut found: Vec<ScreenDef> = Vec::with_capacity(files.len());
    for file in files {
        let relative = file.strip_prefix(package_dir).unwrap_or(&file).to_path_buf();
        let text = tokio::fs::read_to_string(&file)
            .await
            .map_err(|source| ShellDefError::Io { path: file.clone(), source })?;
        let def = parse_screen(&text, &relative)?;
        if found.iter().any(|existing| existing.name == def.name) {
            return Err(ShellDefError::Invalid {
                path: relative,
                reason: format!("screen `{}` is declared more than once", def.name),
            });
        }
        found.push(def);
    }
    Ok(found)
}

pub fn parse_screen(xml: &str, source: &Path) -> Result<ScreenDef, ShellDefError> {
    let invalid = |reason: String| ShellDefError::Invalid { path: source.to_path_buf(), reason };
    let root = compile_xml(xml, source).map_err(|error| invalid(error.to_string()))?;
    let definition = screen(root).map_err(invalid)?;
    let name = definition["name"].as_str().unwrap_or_default().to_string();
    let file_stem = source.file_stem().and_then(|stem| stem.to_str()).unwrap_or_default();
    if file_stem != name {
        return Err(ShellDefError::Invalid {
            path: source.to_path_buf(),
            reason: format!("the file is named `{file_stem}.xml` but the screen is `{name}`"),
        });
    }
    Ok(ScreenDef { name, source: source.to_path_buf(), definition })
}

fn screen(root: Node) -> Result<Value, String> {
    if root.tag != "screen" {
        return Err(format!("the root element must be `<screen>`, not `<{}>`", root.tag));
    }
    let name = root.attribute("name").unwrap_or_default().to_string();
    if !SCREEN_NAMES.contains(&name.as_str()) {
        return Err(format!("`name` must be one of {}", SCREEN_NAMES.join(", ")));
    }
    for (key, _) in &root.attributes {
        if !["name", "layout"].contains(&key.as_str()) {
            return Err(format!("`<screen>` has no attribute `{key}`"));
        }
    }
    let layout = root.attribute("layout").unwrap_or("split");
    if !LAYOUTS.contains(&layout) {
        return Err(format!("`layout` must be one of {}", LAYOUTS.join(", ")));
    }
    let mut out = Map::new();
    out.insert("name".into(), json!(name));
    out.insert("layout".into(), json!(layout));
    let mut has_main = false;
    for child in root.children {
        let part = child.tag.clone();
        match part.as_str() {
            "aside" | "main" => {
                if out.contains_key(&part) {
                    return Err(format!("`<{part}>` appears twice"));
                }
                has_main |= part == "main";
                let value = panel(child, &part, &name).map_err(|reason| format!("`<{part}>`: {reason}"))?;
                out.insert(part, value);
            }
            _ => return Err(format!("unknown element `<{part}>`; use `<aside>` and `<main>`")),
        }
    }
    if !has_main {
        return Err("a screen needs a `<main>`".into());
    }
    Ok(Value::Object(out))
}

fn panel(node: Node, part: &str, screen: &str) -> Result<Value, String> {
    if node.text.is_some() {
        return Err("holds text; put blocks inside it".into());
    }
    let mut out = Map::new();
    for (key, value) in &node.attributes {
        let ok = match (part, key.as_str()) {
            ("aside", "tone") => TONES.contains(&value.as_str()),
            ("aside", "side") => ["left", "right"].contains(&value.as_str()),
            _ => return Err(format!("no attribute `{key}`")),
        };
        if !ok {
            return Err(format!("`{key}` is not valid"));
        }
        out.insert(key.clone(), json!(value));
    }
    if node.children.len() > MAX_BLOCKS {
        return Err(format!("more than {MAX_BLOCKS} blocks"));
    }
    let blocks = node
        .children
        .into_iter()
        .enumerate()
        .map(|(index, child)| block(child, screen).map_err(|reason| format!("block {index}: {reason}")))
        .collect::<Result<Vec<_>, _>>()?;
    out.insert("blocks".into(), Value::Array(blocks));
    Ok(Value::Object(out))
}

fn check_text(text: &str) -> Result<(), String> {
    if text.chars().count() > MAX_TEXT {
        return Err(format!("text is longer than {MAX_TEXT} characters"));
    }
    Ok(())
}

fn block(node: Node, screen: &str) -> Result<Value, String> {
    let tag = node.tag.clone();
    if !node.children.is_empty() {
        return Err(format!("`<{tag}>` cannot hold other elements"));
    }
    let allowed: &[&str] = match tag.as_str() {
        "brand" => &["label", "show_logo"],
        "heading" => &["level"],
        "text" => &["muted"],
        "image" => &["src", "alt"],
        "link" => &["label", "href"],
        "divider" | "spacer" => &[],
        "login-form" | "oauth-buttons" if screen == "login" => &[],
        "registration-notice" if screen == "register" => &[],
        "forgot-password-notice" if screen == "forgot-password" => &[],
        "reset-password-form" if screen == "reset-password" => &[],
        "login-form"
        | "oauth-buttons"
        | "registration-notice"
        | "forgot-password-notice"
        | "reset-password-form" => {
            return Err(format!("`<{tag}>` does not belong on the `{screen}` screen"));
        }
        _ => return Err(format!("unknown block `<{tag}>`")),
    };
    let mut props = Map::new();
    for (key, value) in &node.attributes {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("`<{tag}>` has no attribute `{key}`"));
        }
        check_text(value)?;
        let ok = match (tag.as_str(), key.as_str()) {
            ("heading", "level") => ["1", "2", "3"].contains(&value.as_str()),
            ("text", "muted") | ("brand", "show_logo") => ["true", "false"].contains(&value.as_str()),
            ("image", "src") => is_asset_reference(value),
            ("link", "href") => is_safe_href(value),
            _ => true,
        };
        if !ok {
            return Err(format!("`{tag} {key}` is not valid"));
        }
        let value = match value.as_str() {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            "1" | "2" | "3" if key == "level" => json!(value.parse::<u8>().unwrap_or(1)),
            _ => json!(value),
        };
        props.insert(key.clone(), value);
    }
    if tag == "image" && !props.contains_key("src") {
        return Err("`<image>` needs `src`".into());
    }
    if tag == "link" && (!props.contains_key("href") || !props.contains_key("label")) {
        return Err("`<link>` needs `href` and `label`".into());
    }
    if let Some(text) = node.text {
        if !["heading", "text"].contains(&tag.as_str()) {
            return Err(format!("`<{tag}>` cannot hold text"));
        }
        check_text(&text)?;
        props.insert("text".into(), json!(text));
    }
    let mut block = Map::new();
    block.insert("type".into(), json!(tag));
    if !props.is_empty() {
        block.insert("props".into(), Value::Object(props));
    }
    Ok(Value::Object(block))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOGIN: &str = r#"<screen name="login" layout="split">
        <aside tone="primary"><brand/><heading level="2">Hi</heading><image src="assets/hero.svg" alt=""/></aside>
        <main><heading level="1">Sign in</heading><login-form/><oauth-buttons/><link href="/register" label="Create account"/></main>
    </screen>"#;

    #[test]
    fn reads_a_login_screen() {
        let def = parse_screen(LOGIN, Path::new("screens/login.xml")).unwrap();
        assert_eq!(def.definition["layout"], "split");
        assert_eq!(def.definition["aside"]["tone"], "primary");
        assert_eq!(def.definition["main"]["blocks"][0]["props"]["text"], "Sign in");
        assert_eq!(def.definition["main"]["blocks"][1]["type"], "login-form");
    }

    #[test]
    fn reads_the_other_screens() {
        for (name, block) in [
            ("forgot-password", "forgot-password-notice"),
            ("reset-password", "reset-password-form"),
            ("register", "registration-notice"),
        ] {
            let xml = format!(r#"<screen name="{name}" layout="center"><main><{block}/></main></screen>"#);
            let path = format!("screens/{name}.xml");
            assert!(parse_screen(&xml, Path::new(&path)).is_ok(), "{name}");
            let wrong = format!(r#"<screen name="login"><main><{block}/></main></screen>"#);
            assert!(parse_screen(&wrong, Path::new("screens/login.xml")).is_err(), "{block} on login");
        }
    }

    #[test]
    fn refuses_bad_screens() {
        let bad = [
            ("screens/login.xml", "<layout/>"),
            ("screens/login.xml", r#"<screen name="login"><aside/></screen>"#),
            ("screens/signup.xml", r#"<screen name="signup"><main/></screen>"#),
            ("screens/login.xml", r#"<screen name="register"><main/></screen>"#),
            ("screens/login.xml", r#"<screen name="login" layout="wall"><main/></screen>"#),
            ("screens/login.xml", r#"<screen name="login"><main><registration-notice/></main></screen>"#),
            ("screens/login.xml", r#"<screen name="login"><main><image src="http://x/y.png"/></main></screen>"#),
            ("screens/login.xml", r#"<screen name="login"><main><link href="javascript:alert(1)" label="x"/></main></screen>"#),
            ("screens/login.xml", r#"<screen name="login"><main><script/></main></screen>"#),
            ("screens/login.xml", r#"<screen name="login"><main><login-form>x</login-form></main></screen>"#),
        ];
        for (path, xml) in bad {
            assert!(parse_screen(xml, Path::new(path)).is_err(), "{xml}");
        }
    }
}
