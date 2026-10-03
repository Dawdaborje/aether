//! Plugin pages: XML files under a plugin's `pages/` folder.
//!
//! A page is declared only by its file: the root element carries the route and
//! visibility, there is no page table in `plugin.toml`.
//!
//! ```xml
//! <page route="/messages" public="true" title="Messages" model="message">
//!   ...
//! </page>
//! ```
//!
//! Pages are parsed when a plugin is loaded, so mistakes surface immediately
//! and the catalog always knows which routes are public.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use quick_xml::{Reader, events::Event};
use serde_json::{Map, Value};
use thiserror::Error;

use crate::app_dir::{AppDir, AppDirError};
use crate::plugin_manager::catalog::sha256_hex;

/// Folder, relative to a plugin package, that holds page XML.
pub const PAGES_DIR: &str = "pages";

const MAX_ROUTE_BYTES: usize = 200;

#[derive(Debug, Error)]
pub enum PageError {
    #[error("filesystem error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{path}: invalid XML: {message}")]
    Xml { path: PathBuf, message: String },

    #[error("{0}: the root element must be <page>")]
    NotAPage(PathBuf),

    #[error("{0}: <page> needs a `route` attribute")]
    MissingRoute(PathBuf),

    #[error("{path}: invalid route `{route}`: {reason}")]
    InvalidRoute {
        path: PathBuf,
        route: String,
        reason: &'static str,
    },

    #[error("{path}: `{attribute}` must be \"true\" or \"false\", not `{value}`")]
    InvalidBoolean {
        path: PathBuf,
        attribute: &'static str,
        value: String,
    },

    #[error("{path}: invalid layout `{layout}`: use lowercase letters, digits, `-` and `_`, starting with a letter")]
    InvalidLayout { path: PathBuf, layout: String },

    #[error("route `{route}` is declared by both {first} and {second} (routes that differ only in parameter names match the same URLs)")]
    DuplicateRoute {
        route: String,
        first: PathBuf,
        second: PathBuf,
    },

    #[error("{0} is a symbolic link; pages must be regular files")]
    Symlink(PathBuf),

    #[error(transparent)]
    AppDir(#[from] AppDirError),

    #[error("failed to serialize compiled page: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// One parsed page.
#[derive(Debug, Clone, PartialEq)]
pub struct PageDocument {
    /// Normalised route (`/messages`, `/chat/{channel}`; no trailing slash
    /// except the root `/`).
    pub route: String,
    /// `true` when the route has `{param}` segments.
    pub is_pattern: bool,
    /// The route with every parameter written `{}`; two routes with the same
    /// shape would match the same URLs, so they cannot coexist.
    pub shape: String,
    pub title: String,
    /// The layout this page asks for (`layout="bare"`), overriding the
    /// theme's; `None` uses the theme's layout.
    pub layout: Option<String>,
    /// Whether anonymous visitors may open the page.
    pub public: bool,
    /// The root `model` attribute, if any.
    pub model: Option<String>,
    /// Every model named by any element of the page (`model="..."`).
    pub models: BTreeSet<String>,
    /// Component tree in the shape the web renderer consumes.
    pub tree: Value,
    /// Path of the XML file relative to the plugin package.
    pub source: PathBuf,
}

#[derive(Debug)]
struct Node {
    tag: String,
    attributes: Vec<(String, String)>,
    children: Vec<Node>,
    text: Option<String>,
}

impl Node {
    fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    fn collect_models(&self, models: &mut BTreeSet<String>) {
        if let Some(model) = self.attribute("model").filter(|model| !model.is_empty()) {
            models.insert(model.to_string());
        }
        for child in &self.children {
            child.collect_models(models);
        }
    }

    /// `{ "type": <tag>, <attributes…>, "children": […], "text": … }`.
    ///
    /// The renderer reads the element name from `type`, so an XML `type`
    /// attribute (as on `<view type="list">`) becomes `viewType`. The literals
    /// `true` and `false` become JSON booleans.
    fn into_json(self) -> Value {
        let mut object = Map::new();
        for (key, value) in self.attributes {
            let key = if key == "type" {
                "viewType".to_string()
            } else {
                key
            };
            let value = match value.as_str() {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                _ => Value::String(value),
            };
            object.insert(key, value);
        }
        object.insert("type".into(), Value::String(self.tag));
        if !self.children.is_empty() {
            object.insert(
                "children".into(),
                Value::Array(self.children.into_iter().map(Node::into_json).collect()),
            );
        }
        if let Some(text) = self.text {
            object.insert("text".into(), Value::String(text));
        }
        Value::Object(object)
    }
}

/// Parse one page file. `source` is only used in messages and stored on the result.
pub fn parse_page(xml: &str, source: &Path) -> Result<PageDocument, PageError> {
    let root = compile_xml(xml, source)?;
    if root.tag != "page" {
        return Err(PageError::NotAPage(source.to_path_buf()));
    }

    let route = root
        .attribute("route")
        .ok_or_else(|| PageError::MissingRoute(source.to_path_buf()))?;
    let pattern = RoutePattern::parse(route).map_err(|reason| PageError::InvalidRoute {
        path: source.to_path_buf(),
        route: route.to_string(),
        reason,
    })?;
    let route = pattern.canonical();

    let public = match root.attribute("public") {
        None | Some("false") => false,
        Some("true") => true,
        Some(other) => {
            return Err(PageError::InvalidBoolean {
                path: source.to_path_buf(),
                attribute: "public",
                value: other.to_string(),
            });
        }
    };

    let layout = match root.attribute("layout") {
        None | Some("") => None,
        Some(layout) if super::themes::is_identifier(layout) => Some(layout.to_string()),
        Some(layout) => {
            return Err(PageError::InvalidLayout {
                path: source.to_path_buf(),
                layout: layout.to_string(),
            });
        }
    };
    let title = root
        .attribute("title")
        .filter(|title| !title.is_empty())
        .unwrap_or(&route)
        .to_string();
    let model = root
        .attribute("model")
        .filter(|model| !model.is_empty())
        .map(str::to_string);
    let mut models = BTreeSet::new();
    root.collect_models(&mut models);

    Ok(PageDocument {
        is_pattern: pattern.is_pattern(),
        shape: pattern.shape(),
        route,
        title,
        public,
        layout,
        model,
        models,
        tree: root.into_json(),
        source: source.to_path_buf(),
    })
}

/// Find and parse every `*.xml` under `<package_dir>/pages`, rejecting
/// duplicate routes. Returns nothing when the folder does not exist.
pub async fn discover_pages(package_dir: &Path) -> Result<Vec<PageDocument>, PageError> {
    let pages_dir = package_dir.join(PAGES_DIR);
    let mut files = Vec::new();
    let mut pending = vec![pages_dir.clone()];
    while let Some(directory) = pending.pop() {
        let mut entries = match tokio::fs::read_dir(&directory).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && directory == pages_dir => {
                return Ok(Vec::new());
            }
            Err(source) => {
                return Err(PageError::Io {
                    path: directory,
                    source,
                });
            }
        };
        while let Some(entry) = entries.next_entry().await.map_err(|source| PageError::Io {
            path: directory.clone(),
            source,
        })? {
            let path = entry.path();
            let file_type = entry.file_type().await.map_err(|source| PageError::Io {
                path: path.clone(),
                source,
            })?;
            if file_type.is_symlink() {
                return Err(PageError::Symlink(path));
            }
            if file_type.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "xml") {
                files.push(path);
            }
        }
    }
    files.sort();

    let mut pages: Vec<PageDocument> = Vec::with_capacity(files.len());
    for file in files {
        let relative = file.strip_prefix(package_dir).unwrap_or(&file).to_path_buf();
        let xml = tokio::fs::read_to_string(&file)
            .await
            .map_err(|source| PageError::Io {
                path: file.clone(),
                source,
            })?;
        let page = parse_page(&xml, &relative)?;
        if let Some(existing) = pages.iter().find(|existing| existing.shape == page.shape) {
            return Err(PageError::DuplicateRoute {
                route: page.route,
                first: existing.source.clone(),
                second: page.source,
            });
        }
        pages.push(page);
    }
    Ok(pages)
}

/// Write each page to `<app_dir>/views/<name>/<version>/` as JSON. Files are
/// written under a temporary name and renamed, so readers never see a partial
/// document.
pub async fn write_view_files(
    layout: &AppDir,
    plugin_name: &str,
    version: &str,
    pages: &[PageDocument],
) -> Result<(), PageError> {
    let views_dir = layout.views_dir(plugin_name, version)?;
    tokio::fs::create_dir_all(&views_dir)
        .await
        .map_err(|source| PageError::Io {
            path: views_dir.clone(),
            source,
        })?;

    for page in pages {
        let unique = sha256_hex(format!("{plugin_name}@{version}:{}", page.route).as_bytes());
        let slug: String = page
            .route
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let slug = match slug.trim_matches('_') {
            "" => "root",
            trimmed => trimmed,
        };
        let stem = format!("{slug}-{}", unique.get(..12).unwrap_or(&unique));
        let file = views_dir.join(format!("{stem}.json"));
        let temporary = views_dir.join(format!("{stem}.json.tmp"));

        let document = serde_json::json!({
            "plugin": plugin_name,
            "version": version,
            "route": page.route,
            "title": page.title,
            "public": page.public,
            "layout": page.layout,
            "model": page.model,
            "models": page.models,
            "source": page.source,
            "component_tree": page.tree,
        });
        tokio::fs::write(&temporary, serde_json::to_vec_pretty(&document)?)
            .await
            .map_err(|source| PageError::Io {
                path: temporary.clone(),
                source,
            })?;
        tokio::fs::rename(&temporary, &file)
            .await
            .map_err(|source| PageError::Io { path: file, source })?;
    }
    Ok(())
}

const MAX_PARAM_VALUE_BYTES: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Literal(String),
    Param(String),
}

/// A page route: absolute, `/`-separated segments. A segment is either literal
/// text (letters, digits and `- _ .`) or a single-segment parameter written
/// `{name}` (lowercase letters, digits and `_`, starting with a letter), as in
/// `/chat/{channel}`. A trailing slash is ignored and the root is `/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutePattern {
    segments: Vec<Segment>,
}

impl RoutePattern {
    pub fn parse(raw: &str) -> Result<Self, &'static str> {
        if !raw.starts_with('/') {
            return Err("must start with `/`");
        }
        if raw.len() > MAX_ROUTE_BYTES {
            return Err("is longer than 200 bytes");
        }
        let trimmed = raw.trim_end_matches('/');
        let mut segments = Vec::new();
        let mut names: Vec<&str> = Vec::new();
        for segment in trimmed.split('/').skip(1) {
            if segment.is_empty() {
                return Err("has an empty path segment");
            }
            if let Some(name) = segment.strip_prefix('{').and_then(|rest| rest.strip_suffix('}')) {
                let mut characters = name.chars();
                let valid = characters.next().is_some_and(|first| first.is_ascii_lowercase())
                    && characters.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                    && name.len() <= 32;
                if !valid {
                    return Err("parameter names are lowercase letters, digits and `_`, starting with a letter");
                }
                if names.contains(&name) {
                    return Err("repeats a parameter name");
                }
                names.push(name);
                segments.push(Segment::Param(name.to_string()));
            } else if segment.contains(['{', '}']) {
                return Err("a parameter must be a whole path segment, like `{id}`");
            } else if !segment
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            {
                return Err("may only contain letters, digits and `/ - _ .` (and `{param}` segments)");
            } else if segment == "." || segment == ".." {
                return Err("has a `.` or `..` path segment");
            } else {
                segments.push(Segment::Literal(segment.to_string()));
            }
        }
        Ok(Self { segments })
    }

    /// The route as written canonically: no trailing slash, root as `/`.
    pub fn canonical(&self) -> String {
        self.render(|segment| match segment {
            Segment::Literal(text) => text.clone(),
            Segment::Param(name) => format!("{{{name}}}"),
        })
    }

    /// The route with every parameter as `{}`.
    pub fn shape(&self) -> String {
        self.render(|segment| match segment {
            Segment::Literal(text) => text.clone(),
            Segment::Param(_) => "{}".to_string(),
        })
    }

    fn render(&self, part: impl Fn(&Segment) -> String) -> String {
        if self.segments.is_empty() {
            return "/".to_string();
        }
        self.segments
            .iter()
            .map(|segment| format!("/{}", part(segment)))
            .collect()
    }

    pub fn is_pattern(&self) -> bool {
        self.segments.iter().any(|segment| matches!(segment, Segment::Param(_)))
    }

    /// How many literal segments the route has; among patterns that match a
    /// URL, the one with more literals is the more specific.
    pub fn literal_segments(&self) -> usize {
        self.segments
            .iter()
            .filter(|segment| matches!(segment, Segment::Literal(_)))
            .count()
    }

    /// Match a request path, returning the captured parameters. A parameter
    /// matches one non-empty segment.
    pub fn matches(&self, path: &str) -> Option<BTreeMap<String, String>> {
        let trimmed = path.trim_end_matches('/');
        let parts: Vec<&str> = if trimmed.is_empty() {
            Vec::new()
        } else {
            trimmed.strip_prefix('/')?.split('/').collect()
        };
        if parts.len() != self.segments.len() {
            return None;
        }
        let mut params = BTreeMap::new();
        for (segment, part) in self.segments.iter().zip(parts) {
            match segment {
                Segment::Literal(text) if text == part => {}
                Segment::Literal(_) => return None,
                Segment::Param(name) => {
                    if part.is_empty()
                        || part.len() > MAX_PARAM_VALUE_BYTES
                        || part.contains(['{', '}'])
                    {
                        return None;
                    }
                    params.insert(name.clone(), part.to_string());
                }
            }
        }
        Some(params)
    }
}

fn compile_xml(xml: &str, source: &Path) -> Result<Node, PageError> {
    let xml_error = |message: String| PageError::Xml {
        path: source.to_path_buf(),
        message,
    };
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut stack: Vec<Node> = Vec::new();
    let mut root: Option<Node> = None;

    let read_attributes = |element: &quick_xml::events::BytesStart<'_>,
                           decoder: quick_xml::encoding::Decoder|
     -> Result<Vec<(String, String)>, PageError> {
        let mut attributes = Vec::new();
        for attribute in element.attributes() {
            let attribute = attribute.map_err(|error| xml_error(error.to_string()))?;
            let key = String::from_utf8_lossy(attribute.key.as_ref()).into_owned();
            let value = attribute
                .decode_and_unescape_value(decoder)
                .map_err(|error| xml_error(error.to_string()))?
                .into_owned();
            attributes.push((key, value));
        }
        Ok(attributes)
    };
    let append = |stack: &mut Vec<Node>, root: &mut Option<Node>, node: Node| {
        if let Some(parent) = stack.last_mut() {
            parent.children.push(node);
            Ok(())
        } else if root.replace(node).is_some() {
            Err(xml_error("a page must have exactly one root element".into()))
        } else {
            Ok(())
        }
    };

    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => {
                let attributes = read_attributes(&element, reader.decoder())?;
                stack.push(Node {
                    tag: String::from_utf8_lossy(element.name().as_ref()).into_owned(),
                    attributes,
                    children: Vec::new(),
                    text: None,
                });
            }
            Ok(Event::Empty(element)) => {
                let attributes = read_attributes(&element, reader.decoder())?;
                let node = Node {
                    tag: String::from_utf8_lossy(element.name().as_ref()).into_owned(),
                    attributes,
                    children: Vec::new(),
                    text: None,
                };
                append(&mut stack, &mut root, node)?;
            }
            Ok(Event::End(_)) => {
                let node = stack
                    .pop()
                    .ok_or_else(|| xml_error("unexpected closing element".into()))?;
                append(&mut stack, &mut root, node)?;
            }
            Ok(Event::Text(text)) => {
                let text = text.decode().map_err(|error| xml_error(error.to_string()))?;
                if let Some(node) = stack.last_mut()
                    && !text.trim().is_empty()
                {
                    node.text
                        .get_or_insert_with(String::new)
                        .push_str(text.as_ref());
                }
            }
            Ok(Event::CData(text)) => {
                let text = text.decode().map_err(|error| xml_error(error.to_string()))?;
                if let Some(node) = stack.last_mut() {
                    node.text
                        .get_or_insert_with(String::new)
                        .push_str(text.as_ref());
                }
            }
            Ok(Event::GeneralRef(reference)) => {
                let resolved = resolve_general_ref(&reference).map_err(xml_error)?;
                if let Some(node) = stack.last_mut() {
                    node.text.get_or_insert_with(String::new).push(resolved);
                }
            }
            Ok(Event::Eof) => break,
            Ok(Event::Decl(_) | Event::Comment(_) | Event::DocType(_) | Event::PI(_)) => {}
            Err(error) => return Err(xml_error(error.to_string())),
        }
    }

    if !stack.is_empty() {
        return Err(xml_error("unclosed elements remain".into()));
    }
    root.ok_or_else(|| xml_error("the file has no root element".into()))
}

/// Resolve a character reference (`&#38;`) or one of the five predefined XML
/// entities.
fn resolve_general_ref(reference: &quick_xml::events::BytesRef<'_>) -> Result<char, String> {
    if let Some(character) = reference
        .resolve_char_ref()
        .map_err(|error| error.to_string())?
    {
        return Ok(character);
    }
    let name = reference.decode().map_err(|error| error.to_string())?;
    match name.as_ref() {
        "lt" => Ok('<'),
        "gt" => Ok('>'),
        "amp" => Ok('&'),
        "apos" => Ok('\''),
        "quot" => Ok('"'),
        other => Err(format!("unknown XML entity `&{other};`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(xml: &str) -> Result<PageDocument, PageError> {
        parse_page(xml, Path::new("pages/test.xml"))
    }

    #[test]
    fn reads_route_visibility_title_and_models() -> Result<(), PageError> {
        let page = parse(
            r#"<page route="/messages/" public="true" title="Messages" model="message">
  <view type="list" model="message"><column field="body" sortable="true" /></view>
  <view type="form" model="channel" />
</page>"#,
        )?;
        assert_eq!(page.route, "/messages");
        assert!(page.public);
        assert_eq!(page.title, "Messages");
        assert_eq!(page.model.as_deref(), Some("message"));
        assert_eq!(
            page.models.iter().map(String::as_str).collect::<Vec<_>>(),
            ["channel", "message"]
        );
        Ok(())
    }

    #[test]
    fn emits_the_renderer_shape() -> Result<(), PageError> {
        let page = parse(
            r#"<page route="/x"><view type="list"><column sortable="true" label="A &amp; B" /></view></page>"#,
        )?;
        assert_eq!(page.tree["type"], "page");
        let view = &page.tree["children"][0];
        assert_eq!(view["type"], "view");
        assert_eq!(view["viewType"], "list");
        let column = &view["children"][0];
        assert_eq!(column["sortable"], true);
        assert_eq!(column["label"], "A & B");
        Ok(())
    }

    #[test]
    fn pages_are_private_unless_declared_public() -> Result<(), PageError> {
        assert!(!parse(r#"<page route="/x" />"#)?.public);
        assert!(!parse(r#"<page route="/x" public="false" />"#)?.public);
        Ok(())
    }

    #[test]
    fn parses_parameterised_routes() -> Result<(), PageError> {
        let page = parse(r#"<page route="/chat/{channel}/" />"#)?;
        assert_eq!(page.route, "/chat/{channel}");
        assert!(page.is_pattern);
        assert_eq!(page.shape, "/chat/{}");
        assert!(!parse(r#"<page route="/chat/general" />"#)?.is_pattern);
        Ok(())
    }

    #[test]
    fn patterns_match_one_segment_per_parameter() -> Result<(), &'static str> {
        let pattern = RoutePattern::parse("/chat/{channel}/messages/{id}")?;
        let params = pattern.matches("/chat/general/messages/42").ok_or("should match")?;
        assert_eq!(params["channel"], "general");
        assert_eq!(params["id"], "42");

        assert!(pattern.matches("/chat/general/messages").is_none(), "too short");
        assert!(pattern.matches("/chat/general/messages/42/x").is_none(), "too long");
        assert!(pattern.matches("/chat//messages/42").is_none(), "empty parameter");
        assert!(pattern.matches("/room/general/messages/42").is_none(), "literal differs");
        assert!(pattern.matches("/chat/general/messages/42/").is_some(), "trailing slash ignored");
        assert!(
            pattern.matches("/chat/{channel}/messages/42").is_none(),
            "the pattern's own text is not a value"
        );

        let root = RoutePattern::parse("/")?;
        assert!(root.matches("/").is_some());
        assert!(root.matches("/x").is_none());
        assert_eq!(pattern.literal_segments(), 2);
        Ok(())
    }

    #[test]
    fn a_page_may_name_its_layout() -> Result<(), PageError> {
        assert_eq!(parse(r#"<page route="/" layout="bare" />"#)?.layout.as_deref(), Some("bare"));
        assert_eq!(parse(r#"<page route="/" />"#)?.layout, None);
        assert!(matches!(
            parse(r#"<page route="/" layout="Not Valid" />"#),
            Err(PageError::InvalidLayout { .. })
        ));
        Ok(())
    }

    #[test]
    fn root_route_is_allowed() -> Result<(), PageError> {
        assert_eq!(parse(r#"<page route="/" />"#)?.route, "/");
        Ok(())
    }

    #[test]
    fn rejects_bad_pages() {
        assert!(matches!(parse("<view route=\"/x\" />"), Err(PageError::NotAPage(_))));
        assert!(matches!(parse("<page />"), Err(PageError::MissingRoute(_))));
        for route in [
            "x", "/a b", "/a//b", "/a/../b", "/a?b", "/a{b}", "/{}", "/{Id}", "/{1a}", "/{a}/{a}", "/{a-b}",
        ] {
            let xml = format!(r#"<page route="{route}" />"#);
            assert!(
                matches!(parse(&xml), Err(PageError::InvalidRoute { .. })),
                "{route}"
            );
        }
        for value in ["yes", "True", "1", ""] {
            let xml = format!(r#"<page route="/x" public="{value}" />"#);
            assert!(
                matches!(parse(&xml), Err(PageError::InvalidBoolean { .. })),
                "{value:?}"
            );
        }
        assert!(matches!(parse("<page route=\"/x\">"), Err(PageError::Xml { .. })));
        assert!(matches!(
            parse("<page route=\"/x\" /><page route=\"/y\" />"),
            Err(PageError::Xml { .. })
        ));
    }

    #[tokio::test]
    async fn discovers_pages_recursively_and_rejects_duplicates() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = tempfile::tempdir()?;
        let package = root.path();
        tokio::fs::create_dir_all(package.join("pages/admin")).await?;
        tokio::fs::write(package.join("pages/home.xml"), r#"<page route="/" public="true" />"#).await?;
        tokio::fs::write(package.join("pages/admin/users.xml"), r#"<page route="/admin/users" />"#).await?;
        tokio::fs::write(package.join("pages/notes.txt"), "ignored").await?;

        let pages = discover_pages(package).await?;
        assert_eq!(
            pages.iter().map(|p| p.route.as_str()).collect::<Vec<_>>(),
            ["/admin/users", "/"]
        );
        assert_eq!(pages[0].source, Path::new("pages/admin/users.xml"));

        tokio::fs::write(package.join("pages/again.xml"), r#"<page route="/admin/users/" />"#).await?;
        assert!(matches!(
            discover_pages(package).await,
            Err(PageError::DuplicateRoute { .. })
        ));

        // Routes that differ only in parameter names match the same URLs.
        tokio::fs::remove_file(package.join("pages/again.xml")).await?;
        tokio::fs::write(package.join("pages/a.xml"), r#"<page route="/chat/{channel}" />"#).await?;
        tokio::fs::write(package.join("pages/b.xml"), r#"<page route="/chat/{room}" />"#).await?;
        assert!(matches!(
            discover_pages(package).await,
            Err(PageError::DuplicateRoute { .. })
        ));

        let empty = tempfile::tempdir()?;
        assert!(discover_pages(empty.path()).await?.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn writes_view_json() -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let layout = AppDir::new(root.path());
        let page = parse(r#"<page route="/messages" public="true" model="message" />"#)?;
        write_view_files(&layout, "chat", "0.1.0", &[page]).await?;

        let dir = layout.views_dir("chat", "0.1.0")?;
        let mut files = tokio::fs::read_dir(&dir).await?;
        let entry = files.next_entry().await?.ok_or("no view file written")?;
        let written: Value = serde_json::from_slice(&tokio::fs::read(entry.path()).await?)?;
        assert_eq!(written["route"], "/messages");
        assert_eq!(written["public"], true);
        assert_eq!(written["component_tree"]["type"], "page");
        Ok(())
    }
}
