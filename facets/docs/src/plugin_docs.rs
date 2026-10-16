//! A plugin's own documentation: markdown files the plugin ships, turned into pages for the docs site and the
//! MCP server when the plugin is loaded.
//!
//! The author is not trusted. Raw HTML in the markdown is shown as text, the rendered page is sanitized again
//! (only known tags and attributes survive; `javascript:` and similar links are removed), and files, sizes and
//! links are limited and checked, so a package cannot fill the catalog or reach outside its own pages.

use crate::model::{Doc, Status};
use crate::render::render;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet};

pub const MAX_FILES: usize = 100;
pub const MAX_FILE_BYTES: usize = 256 * 1024;
pub const MAX_TOTAL_BYTES: usize = 1024 * 1024;

/// Who may read a plugin's docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    /// Anyone who can reach the site.
    Public,
    /// Any signed-in user. The default: publishing is a choice the author makes.
    #[default]
    Authenticated,
    /// Members of an organization that has the plugin installed.
    Org,
}

impl Visibility {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "public" => Some(Self::Public),
            "authenticated" => Some(Self::Authenticated),
            "org" => Some(Self::Org),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Authenticated => "authenticated",
            Self::Org => "org",
        }
    }
}

/// The docs of one version of one plugin, rendered. This is what the catalog stores.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDocs {
    pub plugin: String,
    pub version: String,
    pub label: String,
    pub visibility: Visibility,
    /// Sorted by `order`. The page with slug `index` (README.md) is the landing page.
    pub pages: Vec<Doc>,
}

impl PluginDocs {
    pub fn page(&self, slug: &str) -> Option<&Doc> {
        let s = slug.trim_matches('/');
        let s = s.strip_suffix(".md").unwrap_or(s);
        let s = s.strip_suffix("/README").unwrap_or(s);
        let s = if s.is_empty() || s == "README" { "index" } else { s };
        self.pages.iter().find(|p| p.slug == s)
    }
}

/// A markdown file of the plugin, with its path relative to the docs folder (`/` separated).
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub path: String,
    pub text: String,
}

/// Checks and renders the docs of a plugin. Every problem found is reported, not only the first.
pub fn build(plugin: &str, version: &str, label: &str, visibility: Visibility, files: &[SourceFile]) -> Result<PluginDocs, Vec<String>> {
    let mut errors = Vec::new();
    if files.is_empty() {
        return Err(vec!["the docs folder has no .md files".into()]);
    }
    if files.len() > MAX_FILES {
        errors.push(format!("{} docs files; at most {MAX_FILES} are allowed", files.len()));
    }
    let total: usize = files.iter().map(|f| f.text.len()).sum();
    if total > MAX_TOTAL_BYTES {
        errors.push(format!("the docs are {} KB in all; at most {} KB are allowed", total / 1024, MAX_TOTAL_BYTES / 1024));
    }

    // Slugs and the path -> slug map links are resolved against.
    let mut by_path: BTreeMap<String, String> = BTreeMap::new();
    let mut taken: HashSet<String> = HashSet::new();
    for f in files {
        if let Some(problem) = path_problem(&f.path) {
            errors.push(format!("{}: {problem}", f.path));
            continue;
        }
        if f.text.len() > MAX_FILE_BYTES {
            errors.push(format!("{}: {} KB; a file may be at most {} KB", f.path, f.text.len() / 1024, MAX_FILE_BYTES / 1024));
        }
        let slug = slug_of(&f.path);
        if !taken.insert(slug.clone()) {
            errors.push(format!("{}: the page address `{slug}` is used by another file too", f.path));
        }
        by_path.insert(f.path.clone(), slug);
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let mut pages = Vec::new();
    for (i, f) in files.iter().enumerate() {
        let (meta, body) = match frontmatter(&f.text) {
            Ok(x) => x,
            Err(e) => {
                errors.push(format!("{}: {e}", f.path));
                continue;
            }
        };
        let slug = slug_of(&f.path);
        // A README is named after its folder (`guide/README.md` is "Guide"); the top one "Overview".
        let stem_title = {
            let mut parts = f.path.trim_end_matches(".md").rsplit('/');
            match (parts.next(), parts.next()) {
                (Some("README"), Some(dir)) => humanize(dir),
                (Some("README"), None) => "Overview".to_string(),
                (Some(name), _) => humanize(name),
                _ => "Untitled".to_string(),
            }
        };
        let first = render(&f.path, body, &stem_title, false, &|t| by_path.get(t).cloned(), &mut errors);
        let title = meta
            .title
            .or_else(|| first.headings.iter().find(|h| h.level == 1).map(|h| h.text.clone()))
            .filter(|t| !t.trim().is_empty())
            .unwrap_or(stem_title);
        // Rendered once for the links, once more with the real title so the first chunk is named right.
        let r = render(&f.path, body, &title, false, &|t| by_path.get(t).cloned(), &mut Vec::new());
        pages.push(Doc {
            slug: slug.clone(),
            title,
            description: meta.description.unwrap_or_default(),
            section: "plugin-docs".into(),
            status: Status::Stable,
            order: meta.order.unwrap_or(if slug == "index" { 0 } else { 1000 + i as u32 }),
            source: f.path.clone(),
            markdown: body.to_string(),
            html: sanitize(&r.html),
            headings: r.headings,
            chunks: r.chunks,
        });
    }
    if !errors.is_empty() {
        errors.sort();
        errors.dedup();
        return Err(errors);
    }
    pages.sort_by(|a, b| (a.order, &a.slug).cmp(&(b.order, &b.slug)));
    Ok(PluginDocs { plugin: plugin.to_string(), version: version.to_string(), label: label.to_string(), visibility, pages })
}

fn path_problem(path: &str) -> Option<&'static str> {
    if !path.ends_with(".md") {
        return Some("is not a .md file");
    }
    if path.starts_with('/') || path.split('/').any(|p| p.is_empty() || p == "." || p == "..") {
        return Some("is not a plain relative path");
    }
    if !path.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/')) {
        return Some("may only use letters, digits and . _ - / in its name");
    }
    None
}

/// `README.md` is `index`, `guide/README.md` is `guide`, `guide/setup.md` is `guide/setup`.
pub fn slug_of(path: &str) -> String {
    let stem = path.strip_suffix(".md").unwrap_or(path);
    match stem {
        "README" => "index".to_string(),
        s if s.ends_with("/README") => s.trim_end_matches("/README").to_string(),
        s => s.to_string(),
    }
}

fn humanize(stem: &str) -> String {
    let words: Vec<String> = stem.split(['_', '-']).filter(|w| !w.is_empty()).map(str::to_string).collect();
    let mut s = words.join(" ");
    if let Some(first) = s.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    if s.is_empty() { "Untitled".into() } else { s }
}

#[derive(Default)]
struct Meta {
    title: Option<String>,
    description: Option<String>,
    order: Option<u32>,
}

/// Optional `---` block of `key: value` lines. Only `title`, `description` and `order` are read; others are ignored.
fn frontmatter(text: &str) -> Result<(Meta, &str), String> {
    let Some(rest) = text.strip_prefix("---\n") else {
        return Ok((Meta::default(), text));
    };
    let Some(end) = rest.find("\n---\n") else {
        return Err("the frontmatter block is not closed with `---`".into());
    };
    let (block, body) = (&rest[..end], &rest[end + 5..]);
    let mut meta = Meta::default();
    for line in block.lines().filter(|l| !l.trim().is_empty()) {
        let (k, v) = line.split_once(':').ok_or_else(|| format!("bad frontmatter line `{line}`"))?;
        let v = v.trim();
        let v = v.strip_prefix('"').and_then(|s| s.strip_suffix('"')).unwrap_or(v).to_string();
        match k.trim() {
            "title" => meta.title = Some(v),
            "description" => meta.description = Some(v),
            "order" => meta.order = Some(v.parse().map_err(|_| "frontmatter `order` is not a whole number".to_string())?),
            _ => {}
        }
    }
    Ok((meta, body))
}

/// Keeps only safe tags and attributes. Page links (`doc:`), heading ids and code language classes survive;
/// scripts, event handlers, `javascript:` links and unknown tags do not.
pub fn sanitize(html: &str) -> String {
    let mut b = ammonia::Builder::default();
    b.add_url_schemes(["doc"]);
    b.add_tag_attributes("code", ["class"]);
    for h in ["h1", "h2", "h3", "h4", "h5", "h6"] {
        b.add_tag_attributes(h, ["id"]);
    }
    b.link_rel(Some("noopener noreferrer nofollow"));
    b.attribute_filter(|element, attribute, value| match (element, attribute) {
        // Only our own heading ids and `language-xyz` classes; an id like `theme-toggle` cannot be injected
        // because the markdown cannot write HTML, and these filters make that hold even if it could.
        ("code", "class") => (value.starts_with("language-") && value[9..].chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '+')).then(|| Cow::Owned(value.to_string())),
        (h, "id") if h.len() == 2 && h.starts_with('h') => (!value.is_empty() && value.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')).then(|| Cow::Owned(value.to_string())),
        _ => Some(Cow::Owned(value.to_string())),
    });
    b.clean(html).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(list: &[(&str, &str)]) -> Vec<SourceFile> {
        list.iter().map(|(p, t)| SourceFile { path: (*p).into(), text: (*t).into() }).collect()
    }

    fn built(list: &[(&str, &str)]) -> PluginDocs {
        build("notes", "1.0.0", "Notes", Visibility::Public, &files(list)).unwrap()
    }

    #[test]
    fn readme_is_the_landing_page_and_titles_fall_back() {
        let d = built(&[
            ("README.md", "# Notes plugin\n\nHello.\n"),
            ("guide/README.md", "Intro to the guide."),
            ("guide/first_steps.md", "---\ntitle: \"Getting going\"\norder: 5\n---\nBody"),
            ("faq.md", "No heading here."),
        ]);
        let slugs: Vec<&str> = d.pages.iter().map(|p| p.slug.as_str()).collect();
        assert_eq!(slugs, ["index", "guide/first_steps", "guide", "faq"]);
        assert_eq!(d.page("README.md").unwrap().title, "Notes plugin");
        assert_eq!(d.page("guide/first_steps").unwrap().title, "Getting going");
        assert_eq!(d.page("guide").unwrap().title, "Guide");
        let overview = built(&[("README.md", "Just text.")]);
        assert_eq!(overview.page("index").unwrap().title, "Overview");
        assert_eq!(d.page("/faq.md").unwrap().title, "Faq");
        assert!(d.page("nope").is_none());
    }

    #[test]
    fn links_between_pages_are_rewritten_and_checked() {
        let d = built(&[("README.md", "See [setup](guide/setup.md#install)."), ("guide/setup.md", "# Setup\n## Install\nBack to [home](../README.md).")]);
        assert!(d.page("index").unwrap().html.contains("href=\"doc:guide/setup#install\""));
        assert!(d.page("guide/setup").unwrap().html.contains("href=\"doc:index\""));

        let e = build("n", "1", "N", Visibility::Public, &files(&[("a.md", "[gone](missing.md) and ![img](pic.png) and [up](../../x.md) and [x](javascript:alert(1)) and [t](notes.txt)")])).unwrap_err();
        assert!(e.iter().any(|m| m.contains("does not exist")), "{e:?}");
        assert!(e.iter().any(|m| m.contains("must be an https:// address")), "{e:?}");
        assert!(e.iter().any(|m| m.contains("leaves the documentation folder")), "{e:?}");
        assert!(e.iter().any(|m| m.contains("unsupported scheme `javascript:`")), "{e:?}");
        assert!(e.iter().any(|m| m.contains("not to a .md page")), "{e:?}");
        // An https image is fine.
        assert!(build("n", "1", "N", Visibility::Public, &files(&[("a.md", "![logo](https://example.com/l.png)")])).is_ok());
    }

    #[test]
    fn untrusted_markup_cannot_run() {
        let d = built(&[(
            "README.md",
            "# T\n\n<script>alert(1)</script>\n\n<img src=x onerror=alert(2)>\n\n<a href=\"javascript:alert(3)\">click</a> [ok](https://example.com)\n\n<div id=\"theme-toggle\">x</div>\n\n```toml\nname = \"x\"\n```\n",
        )]);
        let html = &d.page("index").unwrap().html;
        // No author-written element exists: the markup is visible text, with its `<` escaped.
        for tag in ["<script", "<img", "<div", "<a href=\"javascript"] {
            assert!(!html.contains(tag), "{tag} survived: {html}");
        }
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"), "{html}");
        assert!(html.contains("href=\"https://example.com\"") && html.contains("rel=\"noopener noreferrer nofollow\""), "{html}");
        assert!(html.contains("<h1 id=\"t\">") && html.contains("class=\"language-toml\""), "{html}");
    }

    #[test]
    fn sanitize_alone_removes_dangerous_html() {
        let html = sanitize("<h2 id=\"a b\" onclick=\"x()\">T</h2><a href=\"javascript:x()\">l</a><iframe src=\"//e\"></iframe><code class=\"evil\">c</code><a href=\"doc:x#y\">k</a>");
        assert!(!html.contains("onclick") && !html.contains("javascript") && !html.contains("iframe"), "{html}");
        assert!(!html.contains("id=\"a b\"") && !html.contains("class=\"evil\""), "{html}");
        assert!(html.contains("href=\"doc:x#y\""), "{html}");
    }

    #[test]
    fn limits_and_names_are_enforced() {
        let big = "x".repeat(MAX_FILE_BYTES + 1);
        let e = build("n", "1", "N", Visibility::Public, &files(&[("big.md", &big)])).unwrap_err();
        assert!(e[0].contains("a file may be at most"), "{e:?}");

        let many: Vec<SourceFile> = (0..MAX_FILES + 1).map(|i| SourceFile { path: format!("p{i}.md"), text: "x".into() }).collect();
        assert!(build("n", "1", "N", Visibility::Public, &many).unwrap_err()[0].contains("at most 100"));

        for bad in ["../x.md", "/abs.md", "a b.md", "a//b.md", "x.txt", "é.md"] {
            assert!(build("n", "1", "N", Visibility::Public, &files(&[(bad, "x")])).is_err(), "{bad}");
        }
        let dup = build("n", "1", "N", Visibility::Public, &files(&[("a.md", "x"), ("a/README.md", "y")])).unwrap_err();
        assert!(dup[0].contains("used by another file"), "{dup:?}");
        assert!(build("n", "1", "N", Visibility::Public, &[]).is_err());
        assert!(build("n", "1", "N", Visibility::Public, &files(&[("a.md", "---\ntitle: x\n")])).unwrap_err()[0].contains("not closed"));
    }

    #[test]
    fn chunks_and_headings_are_kept_for_search() {
        let d = built(&[("README.md", "# Notes\n\nIntro.\n\n## Sharing\n\nShare a note with `notes::share`.\n")]);
        let p = d.page("index").unwrap();
        assert!(p.chunks.iter().any(|c| c.heading.contains("Sharing") && c.markdown.contains("notes::share")));
        assert!(p.headings.iter().any(|h| h.id == "sharing"));
    }

    #[test]
    fn visibility_parses() {
        assert_eq!(Visibility::default(), Visibility::Authenticated);
        assert_eq!(Visibility::parse(" Public "), Some(Visibility::Public));
        assert_eq!(Visibility::parse("org"), Some(Visibility::Org));
        assert_eq!(Visibility::parse("everyone"), None);
        assert_eq!(serde_json::to_string(&Visibility::Org).unwrap(), "\"org\"");
    }
}
