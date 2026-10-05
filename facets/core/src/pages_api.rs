//! `GET /api/ui/pages/{route}`: serve a plugin page to the web app.
//!
//! The page is looked up among the plugins installed (and enabled) in the
//! caller's organization. A page marked `public="true"` is served to anyone,
//! issuing an anonymous visitor identity on first contact; any other page needs
//! a logged-in member. Every outcome, including refusals, is recorded in
//! `page_visits`; if that record cannot be written the request fails.

use std::{collections::BTreeMap, net::SocketAddr};

use axum::{
    Extension, Json, Router,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{Value, json};
use surrealdb::types::SurrealValue;

use crate::plugin_manager::pages::RoutePattern;
use crate::access::{
    audit::{Actor, PageVisit, record_page_visit},
    identity::{
        Identity, IdentityError, ensure_visitor, identify, with_retry_after, with_visitor_cookie,
    },
};
use crate::state::AppState;

const MAX_ROUTE_BYTES: usize = 200;

/// Mounted at the router root; the paths are spelled out because the web app
/// requests the root page as `/api/ui/pages/` (empty slug).
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/ui/pages", get(get_root_page))
        .route("/api/ui/pages/", get(get_root_page))
        .route("/api/ui/pages/{*slug}", get(get_page))
}

#[derive(Debug, Deserialize, SurrealValue)]
struct InstalledRow {
    plugin_name: String,
    version: String,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct PageRow {
    /// The page's route as declared (a pattern such as `/chat/{channel}` or a literal path).
    route: String,
    title: String,
    is_public: bool,
    /// The layout the page asks for, overriding the theme's.
    layout: Option<String>,
    component_tree: Value,
    plugin_name: String,
    plugin_version: String,
}

async fn get_root_page(
    state: State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
    jar: CookieJar,
) -> Response {
    serve_page(state.0, peer, headers, uri, jar, "/".to_string()).await
}

async fn get_page(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    jar: CookieJar,
) -> Response {
    let trimmed = slug.trim_matches('/');
    let route = if trimmed.is_empty() {
        "/".to_string()
    } else {
        format!("/{trimmed}")
    };
    serve_page(state, peer, headers, uri, jar, route).await
}

fn failure(jar: CookieJar, status: StatusCode, message: &str) -> Response {
    (status, jar, Json(json!({ "error": message }))).into_response()
}

async fn serve_page(
    state: AppState,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
    jar: CookieJar,
    route: String,
) -> Response {
    with_retry_after(serve_page_inner(state, peer, headers, uri, jar, route).await)
}

async fn serve_page_inner(
    state: AppState,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
    jar: CookieJar,
    route: String,
) -> Response {
    let peer_ip = peer.map(|Extension(ConnectInfo(address))| address.ip());
    let mut identity = match identify(&state, &headers, peer_ip, &uri).await {
        Ok(identity) => identity,
        Err(error) => {
            if matches!(error, IdentityError::Database(_) | IdentityError::Audit(_)) {
                log::error!("page request identity: {error}");
            }
            let (status, message) = error.http();
            return failure(jar, status, message);
        }
    };

    let outcome = resolve(&state, &mut identity, &route).await;
    // The audit row names the page's declared route (the pattern) and, as the
    // path, what was actually requested.
    let (status, plugin, matched_route) = match &outcome {
        Ok(found) => (
            StatusCode::OK,
            Some(found.page.plugin_name.as_str()),
            Some(found.page.route.as_str()),
        ),
        Err(refusal) => (
            refusal.status,
            refusal.plugin.as_deref(),
            refusal.route.as_deref(),
        ),
    };

    let recorded = async {
        let org = state.org(&identity.org_db).await?;
        record_page_visit(
            &org,
            &identity.audit,
            &PageVisit {
                plugin,
                route: matched_route,
                path: &route,
                method: "GET",
                status: status.as_u16(),
            },
        )
        .await
        .map_err(|error| surrealdb::Error::internal(error.to_string()))
    }
    .await;
    if let Err(error) = recorded {
        log::error!("page visit audit failed: {error}");
        return failure(jar, StatusCode::INTERNAL_SERVER_ERROR, "audit unavailable");
    }

    let jar = with_visitor_cookie(&state, &identity, jar);
    match outcome {
        Ok(mut found) => {
            let language = localize(&state, &identity, &headers, &mut found.page).await;
            (
            jar,
            Json(json!({
                "route": found.page.route,
                "path": route,
                "params": found.params,
                "locale": language.locale,
                "dir": language.dir,
                "title": found.page.title,
                "source": found.page.plugin_name,
                "plugin": found.page.plugin_name,
                "version": found.page.plugin_version,
                "public": found.page.is_public,
                "layout": found.page.layout,
                "page": found.page.component_tree,
            })),
        )
            .into_response()
        }
        Err(refusal) => failure(jar, refusal.status, refusal.message),
    }
}

/// The language a page was served in.
struct Language {
    locale: String,
    /// `ltr` or `rtl`.
    dir: &'static str,
}

/// Put the page's `%key%` text into the reader's language. The reader's languages are, best
/// first: for a logged-in member their own, then the organization's, then the server's; a
/// visitor who is not logged in is offered what their browser asks for before the
/// organization's. A page whose plugins have no text for any of them keeps its own words.
async fn localize(state: &AppState, identity: &Identity, headers: &HeaderMap, page: &mut PageRow) -> Language {
    let chain = locale_chain(state, identity, headers);
    let locale = chain.first().cloned().unwrap_or_else(|| "en".to_string());
    let dir = if aether_localization::is_rtl(&locale) { "rtl" } else { "ltr" };

    match translator(state, identity).await {
        Ok(translator) => {
            let plugin = page.plugin_name.clone();
            let mut title = Value::String(std::mem::take(&mut page.title));
            aether_localization::resolve_tree(&mut title, &plugin, &translator, &chain);
            page.title = title.as_str().unwrap_or_default().to_string();
            aether_localization::resolve_tree(&mut page.component_tree, &plugin, &translator, &chain);
        }
        Err(error) => log::warn!("page text not localized: {error}"),
    }
    Language { locale, dir }
}

/// The languages to try for this reader, best first.
pub(crate) fn locale_chain(state: &AppState, identity: &Identity, headers: &HeaderMap) -> Vec<String> {
    let accepted = headers
        .get(axum::http::header::ACCEPT_LANGUAGE)
        .and_then(|value| value.to_str().ok())
        .map(aether_localization::parse_accept_language)
        .unwrap_or_default();
    let server = state.config.i18n.default_locale.as_str();
    let mut sources: Vec<Option<&str>> = Vec::new();
    if !matches!(identity.actor, Actor::User(_)) {
        sources.extend(accepted.iter().map(|locale| Some(locale.as_str())));
    }
    sources.push(Some(server));
    aether_localization::fallback_chain(&sources)
}

/// Put the `%key%` text of a model's labels (model, field and option labels) into the
/// reader's language. `plugin` is the plugin that owns the model.
pub(crate) async fn localize_model_text(
    state: &AppState,
    identity: &Identity,
    headers: &HeaderMap,
    plugin: &str,
    texts: &mut Value,
) {
    let chain = locale_chain(state, identity, headers);
    match translator(state, identity).await {
        Ok(translator) => aether_localization::resolve_tree(texts, plugin, &translator, &chain),
        Err(error) => log::warn!("model text not localized: {error}"),
    }
}

/// The text of the plugins installed in the caller's organization, in install order.
pub(crate) async fn translator(state: &AppState, identity: &Identity) -> Result<aether_localization::Translator, surrealdb::Error> {
    let org = state.org(&identity.org_db).await?;
    let mut response = org
        .query("SELECT plugin_name, version FROM installed_plugins WHERE is_enabled = true ORDER BY installed_at;")
        .await?
        .check()?;
    let installed: Vec<InstalledRow> = response.take(0)?;
    let installed: Vec<(String, String)> =
        installed.into_iter().map(|row| (row.plugin_name, row.version)).collect();
    let core = state.core().await?;
    crate::plugin_manager::i18n::translator_for(&core, &installed).await
}

struct Refusal {
    status: StatusCode,
    message: &'static str,
    /// The plugin and declared route that matched, when one did (for the audit row).
    plugin: Option<String>,
    route: Option<String>,
}

impl Refusal {
    fn new(status: StatusCode, message: &'static str) -> Self {
        Self { status, message, plugin: None, route: None }
    }

    fn for_page(status: StatusCode, message: &'static str, page: &PageRow) -> Self {
        Self {
            status,
            message,
            plugin: Some(page.plugin_name.clone()),
            route: Some(page.route.clone()),
        }
    }
}

/// A page that matched the request and the URL parameters it captured.
struct Found {
    page: PageRow,
    params: BTreeMap<String, String>,
}

fn internal(context: &str, error: impl std::fmt::Display) -> Refusal {
    log::error!("page request {context}: {error}");
    Refusal::new(StatusCode::INTERNAL_SERVER_ERROR, "database error")
}

const PAGE_COLUMNS: &str = "route, title, is_public, layout, component_tree, \
     plugin.name AS plugin_name, plugin.version AS plugin_version";

/// Find the page for `path` among the installed plugins' pages: an exact route
/// first, then the most specific matching `{param}` pattern (the one with the
/// most literal segments).
fn pick_page(
    installed: &[InstalledRow],
    exact: Vec<PageRow>,
    patterns: Vec<PageRow>,
    path: &str,
) -> Option<Found> {
    let is_installed = |page: &PageRow| {
        installed
            .iter()
            .any(|row| row.plugin_name == page.plugin_name && row.version == page.plugin_version)
    };

    if let Some(page) = exact.into_iter().find(|page| is_installed(page)) {
        return Some(Found { page, params: BTreeMap::new() });
    }
    patterns
        .into_iter()
        .filter(|page| is_installed(page))
        .filter_map(|page| {
            let pattern = RoutePattern::parse(&page.route).ok()?;
            let params = pattern.matches(path)?;
            Some((pattern.literal_segments(), page, params))
        })
        .max_by(|(left, a, _), (right, b, _)| {
            // Most literal segments wins; ties fall back to a stable order.
            left.cmp(right).then_with(|| b.route.cmp(&a.route))
        })
        .map(|(_, page, params)| Found { page, params })
}

/// Find the page and decide whether this caller may see it.
async fn resolve(
    state: &AppState,
    identity: &mut Identity,
    route: &str,
) -> Result<Found, Refusal> {
    if route.len() > MAX_ROUTE_BYTES {
        return Err(Refusal::new(StatusCode::NOT_FOUND, "page not found"));
    }

    let org = state.org(&identity.org_db)
        .await
        .map_err(|error| internal("organization selection", error))?;
    let mut response = org
        .query("SELECT plugin_name, version FROM installed_plugins WHERE is_enabled = true;")
        .await
        .and_then(|response| response.check())
        .map_err(|error| internal("installed plugins", error))?;
    let installed: Vec<InstalledRow> = response
        .take(0)
        .map_err(|error| internal("installed plugins decode", error))?;

    let core = state.core()
        .await
        .map_err(|error| internal("core selection", error))?;
    let exact_query = format!(
        "SELECT {PAGE_COLUMNS} FROM plugin_ui_pages WHERE route = $route AND is_pattern = false;"
    );
    let mut response = core
        .query(exact_query)
        .bind(("route", route.to_string()))
        .await
        .and_then(|response| response.check())
        .map_err(|error| internal("page lookup", error))?;
    let exact: Vec<PageRow> = response
        .take(0)
        .map_err(|error| internal("page decode", error))?;

    // Patterns are only fetched when no literal route matched.
    let patterns: Vec<PageRow> = if exact.iter().any(|page| {
        installed
            .iter()
            .any(|row| row.plugin_name == page.plugin_name && row.version == page.plugin_version)
    }) {
        Vec::new()
    } else {
        let pattern_query =
            format!("SELECT {PAGE_COLUMNS} FROM plugin_ui_pages WHERE is_pattern = true;");
        let mut response = core
            .query(pattern_query)
            .await
            .and_then(|response| response.check())
            .map_err(|error| internal("pattern lookup", error))?;
        response
            .take(0)
            .map_err(|error| internal("pattern decode", error))?
    };

    let request_id = identity.audit.request_id.clone();
    let Some(found) = pick_page(&installed, exact, patterns, route) else {
        let installed_names: Vec<&str> = installed.iter().map(|row| row.plugin_name.as_str()).collect();
        // Someone who is not logged in is asked to log in instead of being told
        // what does or does not exist; a logged-in user gets the real answer.
        let refusal = if matches!(identity.actor, Actor::User(_)) {
            Refusal::new(StatusCode::NOT_FOUND, "page not found")
        } else {
            Refusal::new(StatusCode::UNAUTHORIZED, "not authenticated")
        };
        log::info!(
            "{request_id} page `{route}` in organization `{}`: no installed plugin serves it ({} plugin(s) installed: {installed_names:?}) -> {}",
            identity.org_db,
            installed.len(),
            refusal.status
        );
        return Err(refusal);
    };
    log::info!(
        "{request_id} page `{route}` in organization `{}`: plugin {}@{} route `{}` (public: {}, params: {:?})",
        identity.org_db,
        found.page.plugin_name,
        found.page.plugin_version,
        found.page.route,
        found.page.is_public,
        found.params
    );

    if found.page.is_public {
        ensure_visitor(state, identity).await.map_err(|error| match error {
            IdentityError::RateLimited => Refusal::for_page(
                StatusCode::TOO_MANY_REQUESTS,
                "too many requests",
                &found.page,
            ),
            other => internal("visitor creation", other),
        })?;
        return Ok(found);
    }

    // A private page needs a logged-in member. `identify` has already downgraded
    // users who do not belong to the organization.
    if !matches!(identity.actor, Actor::User(_)) {
        let (status, message) = if identity.foreign_user {
            (StatusCode::FORBIDDEN, "not a member of this organization")
        } else {
            (StatusCode::UNAUTHORIZED, "not authenticated")
        };
        log::info!(
            "{request_id} page `{route}` is private; {} cannot see it -> {status}",
            identity.actor.kind()
        );
        return Err(Refusal::for_page(status, message, &found.page));
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(route: &str, plugin: &str) -> PageRow {
        PageRow {
            route: route.to_string(),
            title: route.to_string(),
            is_public: true,
            layout: None,
            component_tree: Value::Null,
            plugin_name: plugin.to_string(),
            plugin_version: "1".to_string(),
        }
    }

    fn installed(plugins: &[&str]) -> Vec<InstalledRow> {
        plugins
            .iter()
            .map(|name| InstalledRow { plugin_name: name.to_string(), version: "1".to_string() })
            .collect()
    }

    #[test]
    fn exact_routes_beat_patterns() {
        let found = pick_page(
            &installed(&["chat"]),
            vec![page("/chat/new", "chat")],
            vec![page("/chat/{channel}", "chat")],
            "/chat/new",
        );
        let found = found.map(|f| (f.page.route, f.params.len()));
        assert_eq!(found, Some(("/chat/new".to_string(), 0)));
    }

    #[test]
    fn the_most_specific_pattern_wins_and_params_are_captured() {
        let patterns = vec![
            page("/{section}/{id}", "chat"),
            page("/chat/{channel}", "chat"),
            page("/chat/{channel}/messages/{id}", "chat"),
        ];
        let found = pick_page(&installed(&["chat"]), vec![], patterns, "/chat/general");
        let found = found.map(|f| (f.page.route, f.params));
        assert_eq!(
            found.as_ref().map(|(route, _)| route.as_str()),
            Some("/chat/{channel}"),
            "two literal-or-param shapes match; the one with a literal wins"
        );
        assert_eq!(
            found.and_then(|(_, params)| params.get("channel").cloned()),
            Some("general".to_string())
        );
    }

    #[test]
    fn pages_of_plugins_not_installed_do_not_match() {
        let found = pick_page(
            &installed(&["other"]),
            vec![],
            vec![page("/chat/{channel}", "chat")],
            "/chat/general",
        );
        assert!(found.is_none());
    }

    #[test]
    fn non_matching_paths_find_nothing() {
        let found = pick_page(
            &installed(&["chat"]),
            vec![],
            vec![page("/chat/{channel}", "chat")],
            "/chat/general/extra",
        );
        assert!(found.is_none());
    }
}
