//! The documentation website: one axum router, served by `aether --serve-docs` on its own and mounted at
//! `/docs` by `aether --serve`. Pages are server-rendered from the embedded bundle, so search works without
//! JavaScript and nothing is read from disk.
//!
//! Links are written with the router's base path (`""` or `"/docs"`), so the same router works at both.
//!
//! Plugin docs come from a [`PluginDocsProvider`], asked per request which docs this viewer may read. A page the
//! viewer may not read is a plain 404, the same as a page that does not exist.

use crate::plugin_docs::{PluginDocs, Visibility};
use crate::provider::PluginDocsProvider;
use crate::{Doc, Hit, SearchOptions, Status, bundle, doc, search, search_in};
use axum::{
    Router,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use std::{collections::HashMap, sync::Arc};

const CSS: &str = include_str!("assets/site.css");
const JS: &str = include_str!("assets/site.js");

/// Where each section sits in the sidebar, and what it is called there.
const SECTIONS: &[(&str, &str)] = &[
    ("guide", "Guide"),
    ("plugin", "Writing plugins"),
    ("architecture", "Architecture"),
    ("capabilities", "Capabilities"),
];

#[derive(Clone)]
struct Site {
    /// `""` when served at the root, `"/docs"` when mounted under the server.
    base: Arc<str>,
    plugins: Option<Arc<dyn PluginDocsProvider>>,
    /// Rendering for static files: nothing that needs a server (no search box).
    export: bool,
}

/// The docs site with Aether's own documentation only. `base` is the path it is served under: `""` (its own
/// server) or e.g. `"/docs"`.
pub fn router(base: &str) -> Router {
    router_with(base, None)
}

/// The docs site, with the docs plugins ship when a provider is given.
pub fn router_with(base: &str, plugins: Option<Arc<dyn PluginDocsProvider>>) -> Router {
    build_router(Site { base: base.trim_end_matches('/').into(), plugins, export: false })
}

/// The site as `export_site` renders it to static files.
pub(crate) fn router_for_export(base: &str) -> Router {
    build_router(Site { base: base.trim_end_matches('/').into(), plugins: None, export: true })
}

fn build_router(site: Site) -> Router {
    Router::new()
        .route("/", get(home))
        .route("/search", get(search_page))
        .route("/llms.txt", get(llms_txt))
        .route("/llms-full.txt", get(llms_full_txt))
        .route("/plugins", get(plugins_index))
        .route("/plugins/{plugin}", get(plugin_landing))
        .route("/plugins/{plugin}/{*slug}", get(plugin_page))
        .route("/assets/site.css", get(|| async { asset("text/css; charset=utf-8", CSS) }))
        .route("/assets/site.js", get(|| async { asset("text/javascript; charset=utf-8", JS) }))
        .route("/{*slug}", get(page))
        .with_state(site)
}

fn asset(content_type: &'static str, body: &'static str) -> Response {
    ([(header::CONTENT_TYPE, content_type), (header::CACHE_CONTROL, "public, max-age=3600")], body).into_response()
}

impl Site {
    /// The plugin docs this viewer may read (none without a provider).
    async fn visible(&self, headers: &HeaderMap) -> Vec<Arc<PluginDocs>> {
        match &self.plugins {
            Some(p) => p.visible(headers).await,
            None => Vec::new(),
        }
    }

    fn plugin_href(&self, plugin: &str, slug: &str) -> String {
        format!("{}/plugins/{}/{}", self.base, plugin, slug)
    }
}

// ---- handlers ---------------------------------------------------------------------------------

async fn home(State(site): State<Site>, headers: HeaderMap) -> Response {
    match doc("index") {
        Some(d) => core_page(&site, &headers, d).await,
        None => not_found(&site, &headers).await,
    }
}

async fn page(State(site): State<Site>, headers: HeaderMap, Path(slug): Path<String>) -> Response {
    // `<page>.md` is the page's markdown, for tools and LLMs that want the source text.
    if slug.ends_with(".md") {
        return match doc(&slug) {
            Some(d) => text_response(&site, "text/markdown; charset=utf-8", d.markdown.clone()),
            None => not_found(&site, &headers).await,
        };
    }
    match doc(&slug) {
        Some(d) => core_page(&site, &headers, d).await,
        None => not_found(&site, &headers).await,
    }
}

async fn core_page(site: &Site, headers: &HeaderMap, d: &Doc) -> Response {
    let nav = site.visible(headers).await;
    let body = page_body(d, &d.html.replace("href=\"doc:", &format!("href=\"{}/", site.base)), None);
    html_page(site, StatusCode::OK, shell(site, &d.title, View::Core(d), &body, &toc(d), &nav))
}

async fn plugins_index(State(site): State<Site>, headers: HeaderMap) -> Response {
    let nav = site.visible(&headers).await;
    let mut body = String::from("<h1>Plugin documentation</h1>");
    if nav.is_empty() {
        body.push_str("<p>No plugin documentation is available to you here.</p>");
    } else {
        body.push_str("<p>Documentation written by the authors of plugins, not by Aether itself.</p><ul class=\"results\">");
        for p in &nav {
            let blurb = p.page("index").map(|d| d.description.as_str()).filter(|d| !d.is_empty()).unwrap_or("");
            body.push_str(&format!(
                "<li><a href=\"{}\">{}</a> <span class=\"badge\">{}</span>{}<p>{}</p></li>",
                site.plugin_href(&p.plugin, "index"),
                esc(&p.label),
                esc(&format!("v{}", p.version)),
                access_badge(p.visibility),
                esc(blurb)
            ));
        }
        body.push_str("</ul>");
    }
    html_page(&site, StatusCode::OK, shell(&site, "Plugin documentation", View::None, &body, "", &nav))
}

async fn plugin_landing(State(site): State<Site>, headers: HeaderMap, Path(plugin): Path<String>) -> Response {
    plugin_response(&site, &headers, &plugin, "index").await
}

async fn plugin_page(State(site): State<Site>, headers: HeaderMap, Path((plugin, slug)): Path<(String, String)>) -> Response {
    plugin_response(&site, &headers, &plugin, &slug).await
}

async fn plugin_response(site: &Site, headers: &HeaderMap, plugin: &str, slug: &str) -> Response {
    let nav = site.visible(headers).await;
    let Some(pd) = nav.iter().find(|p| p.plugin == plugin).cloned() else {
        return not_found_with(site, &nav);
    };
    // The markdown source of a page.
    if slug.ends_with(".md") {
        return match pd.page(slug) {
            Some(d) => text_response(site, "text/markdown; charset=utf-8", d.markdown.clone()),
            None => not_found_with(site, &nav),
        };
    }
    let Some(d) = pd.page(slug) else {
        // A plugin without a README: its landing page is the list of its pages.
        if slug == "index" {
            let mut body = format!("<h1>{}</h1>{}<ul>", esc(&pd.label), plugin_note(site, &pd));
            for p in &pd.pages {
                body.push_str(&format!("<li><a href=\"{}\">{}</a></li>", site.plugin_href(&pd.plugin, &p.slug), esc(&p.title)));
            }
            body.push_str("</ul>");
            return html_page(site, StatusCode::OK, shell(site, &pd.label, View::PluginIndex(&pd), &body, "", &nav));
        }
        return not_found_with(site, &nav);
    };
    let prefix = format!("href=\"{}/plugins/{}/", site.base, pd.plugin);
    let html = d.html.replace("href=\"doc:", &prefix);
    let body = page_body(d, &html, Some(plugin_note(site, &pd)));
    html_page(site, StatusCode::OK, shell(site, &d.title, View::Plugin(&pd, d), &body, &toc(d), &nav))
}

async fn search_page(State(site): State<Site>, headers: HeaderMap, Query(q): Query<HashMap<String, String>>) -> Response {
    let nav = site.visible(&headers).await;
    let query = q.get("q").map(|s| s.trim()).unwrap_or("");
    let opts = SearchOptions { limit: Some(25), ..Default::default() };
    let mut hits: Vec<Hit> = search(query, &opts);
    for p in &nav {
        hits.extend(search_in(&p.pages, query, &opts).into_iter().map(|mut h| {
            h.plugin = Some(p.plugin.clone());
            h
        }));
    }
    hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.slug.cmp(&b.slug)));
    hits.truncate(25);

    let mut body = format!("<h1>Search</h1><p>{} for <strong>{}</strong></p>", plural(hits.len(), "result"), esc(query));
    if !query.is_empty() && hits.is_empty() {
        body.push_str("<p>Nothing matched. Try fewer or different words.</p>");
    }
    body.push_str("<ul class=\"results\">");
    for h in hits {
        let anchor = if h.anchor.is_empty() { String::new() } else { format!("#{}", h.anchor) };
        let badge = if h.status == Status::Proposal { " <span class=\"badge\">proposal</span>" } else { "" };
        let (link, whose) = match &h.plugin {
            Some(plugin) => (
                site.plugin_href(plugin, &h.slug),
                format!("Plugin {} › ", nav.iter().find(|p| &p.plugin == plugin).map(|p| p.label.as_str()).unwrap_or(plugin)),
            ),
            None => (href(&site.base, &h.slug), String::new()),
        };
        body.push_str(&format!(
            "<li><a href=\"{}{}\">{}</a>{}<div class=\"trail\">{}{}</div><p>{}</p></li>",
            esc(&link),
            esc(&anchor),
            esc(&h.title),
            badge,
            esc(&whose),
            esc(&h.heading),
            esc(&h.snippet)
        ));
    }
    body.push_str("</ul>");
    let title = if query.is_empty() { "Search".to_string() } else { format!("Search: {query}") };
    html_page(&site, StatusCode::OK, shell(&site, &title, View::None, &body, "", &nav))
}

async fn llms_txt(State(site): State<Site>, headers: HeaderMap) -> Response {
    let mut out = String::from(
        "# Aether\n\n> Aether is a plugin platform: a Rust kernel runs sandboxed plugins (WebAssembly, Lua, Rhai) over a \
         multi-tenant SurrealDB. These pages explain how to build plugins and how the kernel works. \
         Pages marked (proposal) describe designs that are NOT built yet.\n\n",
    );
    for (section, label) in SECTIONS {
        out.push_str(&format!("## {label}\n\n"));
        for d in bundle().docs.iter().filter(|d| d.section == *section) {
            let mark = if d.status == Status::Proposal { " (proposal, not built)" } else { "" };
            out.push_str(&format!("- [{}]({}/{}.md){}: {}\n", d.title, site.base, d.slug, mark, d.description));
        }
        out.push('\n');
    }
    let plugins = site.visible(&headers).await;
    if !plugins.is_empty() {
        out.push_str("## Plugin documentation\n\nWritten by the authors of each plugin.\n\n");
        for p in &plugins {
            for d in &p.pages {
                out.push_str(&format!("- [{} / {}]({}.md): {}\n", p.label, d.title, site.plugin_href(&p.plugin, &d.slug), d.description));
            }
        }
        out.push('\n');
    }
    out.push_str(&format!("The whole documentation as one file: {}/llms-full.txt\n", site.base));
    text_response(&site, "text/plain; charset=utf-8", out)
}

async fn llms_full_txt(State(site): State<Site>, headers: HeaderMap) -> Response {
    let mut out = String::new();
    for (section, _) in SECTIONS {
        for d in bundle().docs.iter().filter(|d| d.section == *section) {
            out.push_str(&format!("\n\n<!-- page: {} -->\n", d.slug));
            if d.status == Status::Proposal {
                out.push_str("<!-- PROPOSAL: this page describes a design that is not built yet -->\n");
            }
            out.push_str(&d.markdown);
        }
    }
    for p in site.visible(&headers).await {
        for d in &p.pages {
            out.push_str(&format!("\n\n<!-- plugin: {}, version: {}, page: {} -->\n{}", p.plugin, p.version, d.slug, d.markdown));
        }
    }
    text_response(&site, "text/plain; charset=utf-8", out.trim_start().to_string())
}

// ---- rendering --------------------------------------------------------------------------------

enum View<'a> {
    /// A page that is not a document (search, the plugin list, not found).
    None,
    Core(&'a Doc),
    Plugin(&'a PluginDocs, &'a Doc),
    /// The landing list of a plugin without a README.
    PluginIndex(&'a PluginDocs),
}

fn page_body(d: &Doc, html: &str, note: Option<String>) -> String {
    let mut body = String::new();
    if d.status == Status::Proposal {
        body.push_str("<div class=\"banner\"><strong>Proposal.</strong> This page describes a design that is not built yet. It is not available in Aether today.</div>");
    }
    if let Some(note) = note {
        body.push_str(&note);
    }
    // Some pages open with `##`; give those a title too.
    if !d.headings.iter().any(|h| h.level == 1) {
        body.push_str(&format!("<h1>{}</h1>", esc(&d.title)));
    }
    body.push_str(html);
    body
}

fn plugin_note(site: &Site, p: &PluginDocs) -> String {
    format!(
        "<div class=\"note\">Documentation of the plugin <a href=\"{}\"><code>{}</code></a> (version {}), written by its authors. It is not part of Aether itself.</div>",
        site.plugin_href(&p.plugin, "index"),
        esc(&p.plugin),
        esc(&p.version)
    )
}

fn access_badge(v: Visibility) -> &'static str {
    match v {
        Visibility::Public => "",
        Visibility::Authenticated => " <span class=\"badge\">signed-in users</span>",
        Visibility::Org => " <span class=\"badge\">organization members</span>",
    }
}

fn toc(d: &Doc) -> String {
    let items: Vec<String> = d
        .headings
        .iter()
        .filter(|h| (h.level == 2 || h.level == 3) && !h.id.is_empty())
        .map(|h| format!("<li class=\"l{}\"><a href=\"#{}\">{}</a></li>", h.level, esc(&h.id), esc(&h.text)))
        .collect();
    if items.len() > 1 { format!("<aside class=\"toc\"><h4>On this page</h4><ul>{}</ul></aside>", items.join("")) } else { String::new() }
}

fn shell(site: &Site, title: &str, view: View, body: &str, toc: &str, plugins: &[Arc<PluginDocs>]) -> String {
    let base = &*site.base;
    let home = if base.is_empty() { "/" } else { base };
    let description = match &view {
        View::Core(d) | View::Plugin(_, d) => format!("<meta name=\"description\" content=\"{}\">", esc(&d.description)),
        _ => String::new(),
    };
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>{title} · Aether docs</title>{description}\
         <link rel=\"stylesheet\" href=\"{base}/assets/site.css\">\
         <script src=\"{base}/assets/site.js\"></script></head><body>\
         <header class=\"top\"><a class=\"brand\" href=\"{home}\">Aether docs</a>\
         {search}\
         <button class=\"icon\" id=\"theme-toggle\" type=\"button\" aria-label=\"Switch light and dark\">Theme</button></header>\
         <div class=\"layout\">{nav}<main>{body}</main>{toc}</div></body></html>",
        title = esc(title),
        // Searching needs the server; a static copy has no search box.
        search = if site.export {
            "<span style=\"flex:1\"></span>".to_string()
        } else {
            format!("<form action=\"{base}/search\" method=\"get\" role=\"search\"><input type=\"search\" name=\"q\" placeholder=\"Search the docs\" aria-label=\"Search the docs\"></form>")
        },
        nav = sidebar(site, &view, plugins),
    )
}

fn sidebar(site: &Site, view: &View, plugins: &[Arc<PluginDocs>]) -> String {
    let base = &*site.base;
    let core_current = match view {
        View::Core(d) => Some(d.slug.as_str()),
        _ => None,
    };
    let mut out = String::from("<nav class=\"side\" aria-label=\"Documentation\">");
    for (section, label) in SECTIONS {
        let (stable, proposals): (Vec<&Doc>, Vec<&Doc>) =
            bundle().docs.iter().filter(|d| d.section == *section).partition(|d| d.status == Status::Stable);
        if stable.is_empty() && proposals.is_empty() {
            continue;
        }
        let link = |d: &Doc| {
            let active = if Some(d.slug.as_str()) == core_current { " class=\"active\" aria-current=\"page\"" } else { "" };
            format!("<li><a href=\"{}\"{}>{}</a></li>", href(base, &d.slug), active, esc(&nav_label(d)))
        };
        out.push_str(&format!("<h3>{label}</h3><ul>{}</ul>", stable.iter().map(|d| link(d)).collect::<String>()));
        if !proposals.is_empty() {
            let open = if proposals.iter().any(|d| Some(d.slug.as_str()) == core_current) { " open" } else { "" };
            out.push_str(&format!(
                "<details{open}><summary>Proposals (not built)</summary><ul>{}</ul></details>",
                proposals.iter().map(|d| link(d)).collect::<String>()
            ));
        }
    }
    if !plugins.is_empty() {
        let (current_plugin, current_slug) = match view {
            View::Plugin(p, d) => (Some(p.plugin.as_str()), Some(d.slug.as_str())),
            View::PluginIndex(p) => (Some(p.plugin.as_str()), Some("index")),
            _ => (None, None),
        };
        out.push_str(&format!("<h3><a href=\"{base}/plugins\">Plugins</a></h3><ul>"));
        for p in plugins {
            let mine = Some(p.plugin.as_str()) == current_plugin;
            let active = if mine && current_slug == Some("index") { " class=\"active\" aria-current=\"page\"" } else { "" };
            out.push_str(&format!("<li><a href=\"{}\"{}>{}</a>", site.plugin_href(&p.plugin, "index"), active, esc(&p.label)));
            if mine {
                out.push_str("<ul>");
                for d in p.pages.iter().filter(|d| d.slug != "index") {
                    let active = if current_slug == Some(d.slug.as_str()) { " class=\"active\" aria-current=\"page\"" } else { "" };
                    out.push_str(&format!("<li><a href=\"{}\"{}>{}</a></li>", site.plugin_href(&p.plugin, &d.slug), active, esc(&d.title)));
                }
                out.push_str("</ul>");
            }
            out.push_str("</li>");
        }
        out.push_str("</ul>");
    }
    out.push_str("</nav>");
    out
}

/// The sidebar shows short names: the part of a title before a colon.
fn nav_label(d: &Doc) -> String {
    if d.slug == "index" {
        return "Introduction".into();
    }
    match d.title.split_once(':') {
        Some((head, _)) if head.len() > 3 => head.trim().to_string(),
        _ => d.title.clone(),
    }
}

fn href(base: &str, slug: &str) -> String {
    if slug == "index" { format!("{base}/") } else { format!("{base}/{slug}") }
}

async fn not_found(site: &Site, headers: &HeaderMap) -> Response {
    let nav = site.visible(headers).await;
    not_found_with(site, &nav)
}

fn not_found_with(site: &Site, nav: &[Arc<PluginDocs>]) -> Response {
    let body = "<h1>Page not found</h1><p>There is no such page. Try the search box above or pick a page from the menu.</p>";
    html_page(site, StatusCode::NOT_FOUND, shell(site, "Not found", View::None, body, "", nav))
}

fn html_page(site: &Site, status: StatusCode, body: String) -> Response {
    let mut res = (status, body).into_response();
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
    // Same-origin only: the documentation needs no third-party scripts, styles or frames.
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data: https:; form-action 'self'; base-uri 'none'; frame-ancestors 'none'"),
    );
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    private_if_per_viewer(site, h);
    res
}

fn text_response(site: &Site, content_type: &'static str, body: String) -> Response {
    let mut res = ([(header::CONTENT_TYPE, content_type), (header::X_CONTENT_TYPE_OPTIONS, "nosniff")], body).into_response();
    private_if_per_viewer(site, res.headers_mut());
    res
}

/// With plugin docs the answer depends on who asks, so no shared cache may keep it.
fn private_if_per_viewer(site: &Site, h: &mut HeaderMap) {
    if site.plugins.is_some() {
        h.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-cache"));
        h.insert(header::VARY, HeaderValue::from_static("Cookie, Authorization"));
    }
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 { format!("1 {word}") } else { format!("{n} {word}s") }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_docs::{SourceFile, build};
    use crate::provider::StaticDocs;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn get(base: &str, path: &str) -> (StatusCode, axum::http::HeaderMap, String) {
        fetch(router(base), path).await
    }

    async fn fetch(app: Router, path: &str) -> (StatusCode, axum::http::HeaderMap, String) {
        let res = app.oneshot(Request::get(path).body(Body::empty()).unwrap()).await.unwrap();
        let (status, headers) = (res.status(), res.headers().clone());
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
    }

    #[tokio::test]
    async fn serves_home_and_pages() {
        let (status, headers, body) = get("", "/").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("Aether docs") && body.contains("<nav class=\"side\""));
        assert!(headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap().contains("script-src 'self'"));
        assert!(headers.get(header::CACHE_CONTROL).is_none(), "core docs alone are cacheable");

        let (status, _, body) = get("", "/plugin/pages").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("<h1") && body.contains("class=\"active\""));
        assert!(body.contains("On this page"));
        assert!(!body.contains(">Plugins</a></h3>"), "no plugin section without a provider");
    }

    #[tokio::test]
    async fn base_path_is_used_in_every_link() {
        let (_, _, body) = get("/docs", "/architecture").await;
        assert!(body.contains("href=\"/docs/architecture/models\""));
        assert!(body.contains("href=\"/docs/assets/site.css\""));
        assert!(!body.contains("href=\"doc:"), "an unrewritten page link");
        assert!(!body.contains("href=\"/architecture"), "a link that ignores the base path");
    }

    #[tokio::test]
    async fn markdown_and_llms_files() {
        let (status, headers, body) = get("", "/plugin/pages.md").await;
        assert_eq!(status, StatusCode::OK);
        assert!(headers[header::CONTENT_TYPE].to_str().unwrap().starts_with("text/markdown"));
        assert!(body.starts_with("# Pages and data"));

        let (_, _, index) = get("/docs", "/llms.txt").await;
        assert!(index.contains("[Models](/docs/architecture/models.md)"));
        assert!(index.contains("(proposal, not built)"));
        assert!(index.contains("](/docs/index.md)"), "the home page entry");
        assert!(!index.contains("](.md)"));
        let (_, _, full) = get("", "/llms-full.txt").await;
        assert!(full.contains("<!-- page: plugin/commands -->") && full.len() > 200_000);
    }

    #[tokio::test]
    async fn proposals_are_flagged() {
        let (_, _, body) = get("", "/architecture/history").await;
        assert!(body.contains("class=\"banner\""));
        let (_, _, body) = get("", "/architecture/models").await;
        assert!(!body.contains("class=\"banner\""));
    }

    #[tokio::test]
    async fn search_escapes_input_and_finds_pages() {
        let (status, _, body) = get("", "/search?q=scheduler").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("architecture/scheduler"));
        let (_, _, body) = get("", "/search?q=%3Cscript%3Ealert(1)%3C/script%3E").await;
        assert!(!body.contains("<script>alert"));
    }

    #[tokio::test]
    async fn unknown_page_is_404() {
        let (status, _, body) = get("", "/no/such/page").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body.contains("Page not found"));
        assert_eq!(get("", "/no/such/page.md").await.0, StatusCode::NOT_FOUND);
    }

    /// What `aether --serve` does: nest the site into the server's router at `[docs] path`.
    #[tokio::test]
    async fn mounts_under_a_path_next_to_other_routes() {
        let app = Router::new().route("/api/ping", axum::routing::get(|| async { "pong" })).nest_service("/docs", router("/docs"));
        let fetch = |path: &'static str| {
            let app = app.clone();
            async move {
                let res = app.oneshot(Request::get(path).body(Body::empty()).unwrap()).await.unwrap();
                (res.status(), String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap())
            }
        };
        assert_eq!(fetch("/api/ping").await, (StatusCode::OK, "pong".to_string()));
        let (status, body) = fetch("/docs/plugin/pages").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("href=\"/docs/plugin/commands\""));
        assert_eq!(fetch("/docs").await.0, StatusCode::OK, "/docs without a slash");
        assert_eq!(fetch("/docs/").await.0, StatusCode::OK, "/docs/ with a slash");
        assert_eq!(fetch("/plugin/pages").await.0, StatusCode::NOT_FOUND, "the site must not leak to the root");
    }

    // ---- plugin docs ----

    fn plugin(name: &str, visibility: Visibility, files: &[(&str, &str)]) -> PluginDocs {
        let files: Vec<SourceFile> = files.iter().map(|(p, t)| SourceFile { path: (*p).into(), text: (*t).into() }).collect();
        build(name, "1.2.0", &format!("{} tool", name), visibility, &files).unwrap()
    }

    fn with_plugins(base: &str, docs: Vec<PluginDocs>, public_only: bool) -> Router {
        let provider: Arc<dyn PluginDocsProvider> = Arc::new(if public_only { StaticDocs::public_only(docs) } else { StaticDocs::all(docs) });
        router_with(base, Some(provider))
    }

    fn sample() -> Vec<PluginDocs> {
        vec![
            plugin("notes", Visibility::Public, &[("README.md", "# Notes\n\nTake notes. See [sharing](guide/sharing.md).\n"), ("guide/sharing.md", "# Sharing\n\nUse `notes::share`. Back to [home](../README.md).\n")]),
            plugin("payroll", Visibility::Org, &[("README.md", "# Payroll secrets\n\nOnly for payroll staff.\n")]),
        ]
    }

    #[tokio::test]
    async fn plugin_docs_are_listed_rendered_and_linked_under_the_base() {
        let app = with_plugins("/docs", sample(), false);
        let (status, headers, body) = fetch(app.clone(), "/plugins/notes").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("<h1 id=\"notes\">Notes</h1>") && body.contains("Documentation of the plugin"));
        assert!(body.contains("href=\"/docs/plugins/notes/guide/sharing\""), "{body}");
        assert!(body.contains(">Plugins</a></h3>") && body.contains("notes tool"));
        assert_eq!(headers[header::CACHE_CONTROL], "private, no-cache");

        let (status, _, body) = fetch(app.clone(), "/plugins/notes/guide/sharing").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("href=\"/docs/plugins/notes/index\"") && body.contains("class=\"active\""));

        let (_, _, raw) = fetch(app.clone(), "/plugins/notes/guide/sharing.md").await;
        assert!(raw.starts_with("# Sharing"));
        let (_, _, list) = fetch(app, "/plugins").await;
        assert!(list.contains("notes tool") && list.contains("organization members"));
    }

    #[tokio::test]
    async fn a_hidden_plugin_is_a_plain_404_and_absent_everywhere() {
        let app = with_plugins("", sample(), true); // no sessions: only public docs
        for path in ["/plugins/payroll", "/plugins/payroll/index", "/plugins/payroll/index.md", "/plugins/ghost"] {
            let (status, _, body) = fetch(app.clone(), path).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
            assert!(!body.contains("Payroll") && !body.contains("payroll"), "{path} reveals the plugin: {body}");
        }
        let (_, _, home) = fetch(app.clone(), "/").await;
        assert!(home.contains("notes tool") && !home.contains("payroll"));
        let (_, _, found) = fetch(app.clone(), "/search?q=payroll+secrets").await;
        assert!(!found.contains("Payroll secrets"), "search leaks hidden docs");
        let (_, _, llms) = fetch(app.clone(), "/llms.txt").await;
        assert!(!llms.contains("payroll") && llms.contains("/plugins/notes/index.md"));
        let (_, _, full) = fetch(app, "/llms-full.txt").await;
        assert!(!full.contains("Only for payroll staff") && full.contains("notes::share"));
    }

    #[tokio::test]
    async fn plugin_pages_are_searchable_and_cannot_collide_with_core_pages() {
        let app = with_plugins("", sample(), false);
        let (_, _, found) = fetch(app.clone(), "/search?q=notes%3A%3Ashare").await;
        assert!(found.contains("href=\"/plugins/notes/guide/sharing") && found.contains("Plugin notes tool"), "{found}");
        // The plugin's page slug `index` is not the core `index`.
        let (_, _, core_home) = fetch(app.clone(), "/").await;
        assert!(core_home.contains("Aether Official Documentation"));
        let (_, _, plugin_home) = fetch(app, "/plugins/notes/index").await;
        assert!(!plugin_home.contains("Aether Official Documentation") && plugin_home.contains("Take notes"));
    }

    #[tokio::test]
    async fn a_plugin_without_a_readme_lists_its_pages() {
        let only = vec![plugin("tiny", Visibility::Public, &[("usage.md", "# Usage\n\nHow.\n")])];
        let (status, _, body) = fetch(with_plugins("", only, false), "/plugins/tiny").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("href=\"/plugins/tiny/usage\""), "{body}");
    }
}
