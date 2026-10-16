//! The documentation website: one stateless axum router, served by `aether --serve-docs` on its own
//! and mounted at `/docs` by `aether --serve`. Pages are server-rendered from the embedded bundle, so
//! search works without JavaScript and nothing is read from disk.
//!
//! Links are written with the router's base path (`""` or `"/docs"`), so the same router works at both.

use crate::{Doc, SearchOptions, Status, bundle, doc, search};
use axum::{
    Router,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
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
}

/// The docs site. `base` is the path it is served under: `""` (its own server) or e.g. `"/docs"`.
pub fn router(base: &str) -> Router {
    let site = Site { base: base.trim_end_matches('/').into() };
    Router::new()
        .route("/", get(home))
        .route("/search", get(search_page))
        .route("/llms.txt", get(llms_txt))
        .route("/llms-full.txt", get(llms_full_txt))
        .route("/assets/site.css", get(|| async { asset("text/css; charset=utf-8", CSS) }))
        .route("/assets/site.js", get(|| async { asset("text/javascript; charset=utf-8", JS) }))
        .route("/{*slug}", get(page))
        .with_state(site)
}

fn asset(content_type: &'static str, body: &'static str) -> Response {
    ([(header::CONTENT_TYPE, content_type), (header::CACHE_CONTROL, "public, max-age=3600")], body).into_response()
}

// ---- handlers ---------------------------------------------------------------------------------

async fn home(State(site): State<Site>) -> Response {
    match doc("index") {
        Some(d) => html_page(StatusCode::OK, render_doc(&site, d)),
        None => not_found(&site),
    }
}

async fn page(State(site): State<Site>, Path(slug): Path<String>) -> Response {
    // `<page>.md` is the page's markdown, for tools and LLMs that want the source text.
    if slug.ends_with(".md") {
        return match doc(&slug) {
            Some(d) => text_response("text/markdown; charset=utf-8", d.markdown.clone()),
            None => not_found(&site),
        };
    }
    match doc(&slug) {
        Some(d) => html_page(StatusCode::OK, render_doc(&site, d)),
        None => not_found(&site),
    }
}

async fn search_page(State(site): State<Site>, Query(q): Query<HashMap<String, String>>) -> Response {
    let query = q.get("q").map(|s| s.trim()).unwrap_or("");
    let hits = search(query, &SearchOptions { limit: Some(25), ..Default::default() });
    let mut body = format!("<h1>Search</h1><p>{} for <strong>{}</strong></p>", plural(hits.len(), "result"), esc(query));
    if !query.is_empty() && hits.is_empty() {
        body.push_str("<p>Nothing matched. Try fewer or different words.</p>");
    }
    body.push_str("<ul class=\"results\">");
    for h in hits {
        let anchor = if h.anchor.is_empty() { String::new() } else { format!("#{}", h.anchor) };
        let badge = if h.status == Status::Proposal { " <span class=\"badge\">proposal</span>" } else { "" };
        body.push_str(&format!(
            "<li><a href=\"{}{}\">{}</a>{}<div class=\"trail\">{}</div><p>{}</p></li>",
            href(&site.base, h.slug),
            esc(&anchor),
            esc(h.title),
            badge,
            esc(h.heading),
            esc(&h.snippet)
        ));
    }
    body.push_str("</ul>");
    let title = if query.is_empty() { "Search".to_string() } else { format!("Search: {query}") };
    html_page(StatusCode::OK, shell(&site, &title, None, &body, ""))
}

async fn llms_txt(State(site): State<Site>) -> Response {
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
    out.push_str(&format!("The whole documentation as one file: {}/llms-full.txt\n", site.base));
    text_response("text/plain; charset=utf-8", out)
}

async fn llms_full_txt() -> Response {
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
    text_response("text/plain; charset=utf-8", out.trim_start().to_string())
}

// ---- rendering --------------------------------------------------------------------------------

fn render_doc(site: &Site, d: &Doc) -> String {
    let mut body = String::new();
    if d.status == Status::Proposal {
        body.push_str("<div class=\"banner\"><strong>Proposal.</strong> This page describes a design that is not built yet. It is not available in Aether today.</div>");
    }
    // Some pages open with `##`; give those a title too.
    if !d.headings.iter().any(|h| h.level == 1) {
        body.push_str(&format!("<h1>{}</h1>", esc(&d.title)));
    }
    body.push_str(&d.html.replace("href=\"doc:", &format!("href=\"{}/", site.base)));

    let toc: Vec<String> = d
        .headings
        .iter()
        .filter(|h| (h.level == 2 || h.level == 3) && !h.id.is_empty())
        .map(|h| format!("<li class=\"l{}\"><a href=\"#{}\">{}</a></li>", h.level, esc(&h.id), esc(&h.text)))
        .collect();
    let toc = if toc.len() > 1 { format!("<aside class=\"toc\"><h4>On this page</h4><ul>{}</ul></aside>", toc.join("")) } else { String::new() };
    shell(site, &d.title, Some(d), &body, &toc)
}

fn shell(site: &Site, title: &str, current: Option<&Doc>, body: &str, toc: &str) -> String {
    let base = &*site.base;
    let home = if base.is_empty() { "/" } else { base };
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>{title} · Aether docs</title>{desc}\
         <link rel=\"stylesheet\" href=\"{base}/assets/site.css\">\
         <script src=\"{base}/assets/site.js\"></script></head><body>\
         <header class=\"top\"><a class=\"brand\" href=\"{home}\">Aether docs</a>\
         <form action=\"{base}/search\" method=\"get\" role=\"search\"><input type=\"search\" name=\"q\" placeholder=\"Search the docs\" aria-label=\"Search the docs\"></form>\
         <button class=\"icon\" id=\"theme-toggle\" type=\"button\" aria-label=\"Switch light and dark\">Theme</button></header>\
         <div class=\"layout\">{nav}<main>{body}</main>{toc}</div></body></html>",
        title = esc(title),
        desc = current.map(|d| format!("<meta name=\"description\" content=\"{}\">", esc(&d.description))).unwrap_or_default(),
        nav = sidebar(base, current),
    )
}

fn sidebar(base: &str, current: Option<&Doc>) -> String {
    let current_slug = current.map(|d| d.slug.as_str());
    let mut out = String::from("<nav class=\"side\" aria-label=\"Documentation\">");
    for (section, label) in SECTIONS {
        let (stable, proposals): (Vec<&Doc>, Vec<&Doc>) =
            bundle().docs.iter().filter(|d| d.section == *section).partition(|d| d.status == Status::Stable);
        if stable.is_empty() && proposals.is_empty() {
            continue;
        }
        let link = |d: &Doc| {
            let active = if Some(d.slug.as_str()) == current_slug { " class=\"active\" aria-current=\"page\"" } else { "" };
            format!("<li><a href=\"{}\"{}>{}</a></li>", href(base, &d.slug), active, esc(&nav_label(d)))
        };
        out.push_str(&format!("<h3>{label}</h3><ul>{}</ul>", stable.iter().map(|d| link(d)).collect::<String>()));
        if !proposals.is_empty() {
            let open = if proposals.iter().any(|d| Some(d.slug.as_str()) == current_slug) { " open" } else { "" };
            out.push_str(&format!(
                "<details{open}><summary>Proposals (not built)</summary><ul>{}</ul></details>",
                proposals.iter().map(|d| link(d)).collect::<String>()
            ));
        }
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

fn not_found(site: &Site) -> Response {
    let body = "<h1>Page not found</h1><p>There is no such page. Try the search box above or pick a page from the menu.</p>";
    html_page(StatusCode::NOT_FOUND, shell(site, "Not found", None, body, ""))
}

fn html_page(status: StatusCode, body: String) -> Response {
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
    res
}

fn text_response(content_type: &'static str, body: String) -> Response {
    ([(header::CONTENT_TYPE, content_type), (header::X_CONTENT_TYPE_OPTIONS, "nosniff")], body).into_response()
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
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn get(base: &str, path: &str) -> (StatusCode, axum::http::HeaderMap, String) {
        let res = router(base).oneshot(Request::get(path).body(Body::empty()).unwrap()).await.unwrap();
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

        let (status, _, body) = get("", "/plugin/pages").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("<h1") && body.contains("class=\"active\""));
        assert!(body.contains("On this page"));
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
}
