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

use crate::access::{
    audit::{Actor, PageVisit, record_page_visit},
    identity::{
        Identity, IdentityError, ensure_visitor, identify, with_retry_after, with_visitor_cookie,
    },
};
use crate::plugin_manager::components::{
    collect_uses, expand_tree, registry_for_versions, wrap_page_tree,
};
use crate::plugin_manager::field_hints;
use crate::plugin_manager::installed;
use crate::plugin_manager::page_patch;
use crate::plugin_manager::pages::RoutePattern;
use crate::plugin_manager::surface;
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
    /// Content layout component; expanded into the page body before serve.
    #[serde(default)]
    wrap: Option<String>,
    /// Roles that may open this page (OR). Empty = any member.
    #[serde(default)]
    required_roles: Option<Vec<String>>,
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
            let mut contributions = permitted_contributions(&state, &identity, &mut found).await;
            if let Err(error) =
                expand_page_components(&state, &identity, &mut found.page, &mut contributions).await
            {
                log::error!("page component expand: {error}");
                return failure(
                    jar,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "page could not be built",
                );
            }
            // Each tab is worded and typed by the plugin that wrote it, not by the page's.
            for tab in &mut contributions {
                if let Err(error) = add_field_hints(&state, tab).await {
                    log::warn!("tab field hints for `{}`: {error}", tab.route);
                }
                localize(&state, &identity, &headers, tab).await;
            }
            merge_contributions(&mut found.page.component_tree, contributions);
            // A page that fails to get model hints still works: its fields just keep
            // whatever `fieldType` the author wrote.
            if let Err(error) = add_field_hints(&state, &mut found.page).await {
                log::warn!("page field hints for `{}`: {error}", found.page.route);
            }
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

/// The routes a contributed page says it extends (comma separated); `None` for an ordinary page.
fn extends_of(tree: &Value) -> Option<Vec<&str>> {
    let raw = tree.get("extends").and_then(Value::as_str)?;
    let targets: Vec<&str> = raw
        .split(',')
        .map(str::trim)
        .filter(|target| !target.is_empty())
        .collect();
    (!targets.is_empty()).then_some(targets)
}

/// The contributions this caller may see: those with `roles` they hold (a developer holds all).
async fn permitted_contributions(
    state: &AppState,
    identity: &Identity,
    found: &mut Found,
) -> Vec<PageRow> {
    let all = std::mem::take(&mut found.contributions);
    if all.iter().all(|page| {
        page.required_roles
            .as_deref()
            .is_none_or(<[String]>::is_empty)
    }) {
        return all;
    }
    let is_developer = identity
        .session
        .as_ref()
        .is_some_and(|session| session.user.is_super_user);
    if is_developer {
        return all;
    }
    let Actor::User(user_id) = &identity.actor else {
        return Vec::new();
    };
    let roles = async {
        let org = state.org(&identity.org_db).await.ok()?;
        crate::roles::roles_of_cached(&state.cache, &identity.org_db, &org, user_id)
            .await
            .ok()
    }
    .await;
    let Some(roles) = roles else {
        return Vec::new();
    };
    all.into_iter()
        .filter(|page| {
            let required = page.required_roles.as_deref().unwrap_or(&[]);
            required.is_empty() || crate::roles::allows(&roles, required)
        })
        .collect()
}

/// Add each contribution to `tree` as a tab of its first notebook. When the page has no
/// notebook one is added to its first form view (or to the page itself). The tab carries
/// `plugin`, so the web app calls the contributing plugin's functions from inside it, and
/// `params`, which maps the page's route values onto the names that plugin expects.
fn merge_contributions(tree: &mut Value, mut contributions: Vec<PageRow>) {
    if contributions.is_empty() {
        return;
    }
    let order = |page: &PageRow| {
        page.component_tree
            .get("sort")
            .and_then(Value::as_str)
            .and_then(|sort| sort.parse::<i64>().ok())
            .unwrap_or(0)
    };
    contributions.sort_by(|a, b| {
        order(a)
            .cmp(&order(b))
            .then_with(|| a.plugin_name.cmp(&b.plugin_name))
            .then_with(|| a.title.cmp(&b.title))
    });
    // A contribution with `<xpath>` elements edits the page; any other adds a tab. The
    // edits go first, in the same order as tabs, so a tab sees the page as patched.
    let (patches, contributions): (Vec<PageRow>, Vec<PageRow>) = contributions
        .into_iter()
        .partition(|page| page_patch::is_patch(&page.component_tree));
    for patch in &patches {
        for problem in page_patch::apply(tree, &patch.component_tree) {
            log::warn!(
                "page patch `{}` of plugin `{}`: {problem}",
                patch.title,
                patch.plugin_name
            );
        }
    }
    if contributions.is_empty() {
        return;
    }
    let tabs: Vec<Value> = contributions
        .into_iter()
        .map(|page| {
            let mut tab = serde_json::Map::new();
            tab.insert("type".into(), json!("page"));
            tab.insert("title".into(), json!(page.title));
            tab.insert("plugin".into(), json!(page.plugin_name));
            if let Some(params) = page.component_tree.get("params") {
                tab.insert("params".into(), params.clone());
            }
            if let Some(children) = page.component_tree.get("children") {
                tab.insert("children".into(), children.clone());
            }
            Value::Object(tab)
        })
        .collect();

    fn push_children(node: &mut Value, tabs: Vec<Value>) {
        if let Some(object) = node.as_object_mut() {
            let children = object
                .entry("children")
                .or_insert_with(|| Value::Array(Vec::new()));
            if let Some(list) = children.as_array_mut() {
                list.extend(tabs);
            }
        }
    }
    fn first_of<'a>(node: &'a mut Value, kind: &str) -> Option<&'a mut Value> {
        if node.get("type").and_then(Value::as_str) == Some(kind) {
            return Some(node);
        }
        node.get_mut("children")?
            .as_array_mut()?
            .iter_mut()
            .find_map(|child| first_of(child, kind))
    }

    if let Some(notebook) = first_of(tree, "notebook") {
        push_children(notebook, tabs);
        return;
    }
    let mut notebook = json!({ "type": "notebook", "children": tabs });
    match first_of(tree, "view") {
        Some(view) => push_children(view, vec![notebook.take()]),
        None => push_children(tree, vec![notebook.take()]),
    }
}

/// Tell the page's fields what the model says they are (see `plugin_manager::field_hints`).
async fn add_field_hints(state: &AppState, page: &mut PageRow) -> Result<(), String> {
    let core = state.core().await.map_err(|error| error.to_string())?;
    let rows = surface::models_cached(&state.cache, &core, &page.plugin_name, &page.plugin_version)
        .await
        .map_err(|error| error.to_string())?;
    let models = field_hints::index_models(rows);
    field_hints::annotate_fields(&mut page.component_tree, &models);
    Ok(())
}

/// Resolve page `wrap=` and `<use>` nodes against the components of plugins
/// installed in this organization, so the browser only sees built-in widget tags.
async fn expand_page_components(
    state: &AppState,
    identity: &Identity,
    page: &mut PageRow,
    contributions: &mut [PageRow],
) -> Result<(), String> {
    let wrap = page
        .wrap
        .as_deref()
        .filter(|name| !name.is_empty())
        .map(str::to_string);
    let has_uses = !collect_uses(&page.component_tree).is_empty()
        || contributions
            .iter()
            .any(|tab| !collect_uses(&tab.component_tree).is_empty());
    if wrap.is_none() && !has_uses {
        return Ok(());
    }
    let org = state
        .org(&identity.org_db)
        .await
        .map_err(|error| error.to_string())?;
    let installed = installed::list_cached(&state.cache, &identity.org_db, &org)
        .await
        .map_err(|error| error.to_string())?;
    let installed = installed::enabled_pairs(&installed);

    let core = state.core().await.map_err(|error| error.to_string())?;
    let mut rows = Vec::new();
    for (name, version) in &installed {
        let mut batch = surface::components_cached(&state.cache, &core, name, version)
            .await
            .map_err(|error| error.to_string())?;
        rows.append(&mut batch);
    }
    let registry = registry_for_versions(rows, &installed);
    if let Some(wrap) = wrap.as_deref() {
        page.component_tree = wrap_page_tree(&page.component_tree, wrap, &page.title)
            .map_err(|error| error.to_string())?;
    }
    page.component_tree = expand_tree(&page.component_tree, &page.plugin_name, &registry)
        .map_err(|error| error.to_string())?;
    // A contributed tab resolves its short component names against the plugin that wrote it.
    for tab in contributions {
        tab.component_tree = expand_tree(&tab.component_tree, &tab.plugin_name, &registry)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
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
async fn localize(
    state: &AppState,
    identity: &Identity,
    headers: &HeaderMap,
    page: &mut PageRow,
) -> Language {
    let chain = locale_chain(state, identity, headers);
    let locale = chain.first().cloned().unwrap_or_else(|| "en".to_string());
    let dir = if aether_localization::is_rtl(&locale) {
        "rtl"
    } else {
        "ltr"
    };

    match translator(state, identity).await {
        Ok(translator) => {
            let plugin = page.plugin_name.clone();
            let mut title = Value::String(std::mem::take(&mut page.title));
            aether_localization::resolve_tree(&mut title, &plugin, &translator, &chain);
            page.title = title.as_str().unwrap_or_default().to_string();
            aether_localization::resolve_tree(
                &mut page.component_tree,
                &plugin,
                &translator,
                &chain,
            );
        }
        Err(error) => log::warn!("page text not localized: {error}"),
    }
    Language { locale, dir }
}

/// The languages to try for this reader, best first.
pub(crate) fn locale_chain(
    state: &AppState,
    identity: &Identity,
    headers: &HeaderMap,
) -> Vec<String> {
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
pub(crate) async fn translator(
    state: &AppState,
    identity: &Identity,
) -> Result<aether_localization::Translator, surrealdb::Error> {
    let org = state.org(&identity.org_db).await?;
    let installed = installed::list_cached(&state.cache, &identity.org_db, &org).await?;
    let installed = installed::enabled_pairs(&installed);
    let core = state.core().await?;
    crate::plugin_manager::i18n::translator_for_cached(Some(&state.cache), &core, &installed).await
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
        Self {
            status,
            message,
            plugin: None,
            route: None,
        }
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
    /// Pages of other (or the same) installed plugins that declared `extends` this page's route:
    /// their bodies are added to it as tabs when it is served.
    contributions: Vec<PageRow>,
}

fn internal(context: &str, error: impl std::fmt::Display) -> Refusal {
    log::error!("page request {context}: {error}");
    Refusal::new(StatusCode::INTERNAL_SERVER_ERROR, "database error")
}

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
        return Some(Found {
            page,
            params: BTreeMap::new(),
            contributions: Vec::new(),
        });
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
        .map(|(_, page, params)| Found {
            page,
            params,
            contributions: Vec::new(),
        })
}

/// Find the page and decide whether this caller may see it.
async fn resolve(state: &AppState, identity: &mut Identity, route: &str) -> Result<Found, Refusal> {
    if route.len() > MAX_ROUTE_BYTES {
        return Err(Refusal::new(StatusCode::NOT_FOUND, "page not found"));
    }

    let org = state
        .org(&identity.org_db)
        .await
        .map_err(|error| internal("organization selection", error))?;
    let installed = installed::list_cached(&state.cache, &identity.org_db, &org)
        .await
        .map_err(|error| internal("installed plugins", error))?;
    let installed: Vec<InstalledRow> = installed
        .into_iter()
        .filter(|row| row.is_enabled)
        .map(|row| InstalledRow {
            plugin_name: row.plugin_name,
            version: row.version,
        })
        .collect();

    let core = state
        .core()
        .await
        .map_err(|error| internal("core selection", error))?;
    let mut exact = Vec::new();
    let mut patterns = Vec::new();
    let mut contributions = Vec::new();
    for row in &installed {
        let pages = surface::pages_cached(&state.cache, &core, &row.plugin_name, &row.version)
            .await
            .map_err(|error| internal("page lookup", error))?;
        for page in pages {
            let is_pattern = page.is_pattern;
            let page = PageRow {
                route: page.route,
                title: page.title,
                is_public: page.is_public,
                layout: page.layout,
                wrap: page.wrap,
                required_roles: page.required_roles,
                component_tree: page.component_tree,
                plugin_name: page.plugin_name,
                plugin_version: page.plugin_version,
            };
            // A contribution is never served as a page of its own.
            if extends_of(&page.component_tree).is_some() {
                contributions.push(page);
            } else if !is_pattern && page.route == route {
                exact.push(page);
            } else if is_pattern {
                patterns.push(page);
            }
        }
    }
    // Prefer an installed exact match; only then consider patterns.
    let patterns = if exact.iter().any(|page| {
        installed
            .iter()
            .any(|row| row.plugin_name == page.plugin_name && row.version == page.plugin_version)
    }) {
        Vec::new()
    } else {
        patterns
    };

    let request_id = identity.audit.request_id.clone();
    let found = pick_page(&installed, exact, patterns, route).map(|mut found| {
        if let Ok(host) = RoutePattern::parse(&found.page.route) {
            let shape = host.shape();
            found.contributions = contributions
                .into_iter()
                .filter(|page| {
                    extends_of(&page.component_tree).is_some_and(|targets| {
                        targets.into_iter().any(|target| {
                            RoutePattern::parse(target).is_ok_and(|t| t.shape() == shape)
                        })
                    })
                })
                .collect();
        }
        found
    });
    let Some(found) = found else {
        let installed_names: Vec<&str> = installed
            .iter()
            .map(|row| row.plugin_name.as_str())
            .collect();
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
        ensure_visitor(state, identity)
            .await
            .map_err(|error| match error {
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
    let Actor::User(user_id) = &identity.actor else {
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
    };

    // A page that names no roles needs a role of its own plugin (when the plugin offers any).
    let required = found.page.required_roles.as_deref().unwrap_or(&[]);
    {
        let is_developer = identity
            .session
            .as_ref()
            .map(|session| session.user.is_super_user)
            .unwrap_or(false);
        if !is_developer {
            let org_db = state
                .org(&identity.org_db)
                .await
                .map_err(|error| internal("organization for page roles", error))?;
            let user_roles =
                crate::roles::roles_of_cached(&state.cache, &identity.org_db, &org_db, user_id)
                    .await
                    .map_err(|error| internal("page roles", error))?;
            let org_roles = if required.is_empty() {
                crate::roles::all_names(&org_db)
                    .await
                    .map_err(|error| internal("organization roles", error))?
            } else {
                Vec::new()
            };
            if !crate::roles::allows_in_plugin(
                &user_roles,
                required,
                &found.page.plugin_name,
                &org_roles,
            ) {
                log::info!(
                    "{request_id} page `{route}` needs roles {required:?}; `{user_id}` lacks them -> 403"
                );
                return Err(Refusal::for_page(
                    StatusCode::FORBIDDEN,
                    "you do not have permission to open this page",
                    &found.page,
                ));
            }
        }
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
            wrap: None,
            required_roles: None,
            component_tree: Value::Null,
            plugin_name: plugin.to_string(),
            plugin_version: "1".to_string(),
        }
    }

    fn installed(plugins: &[&str]) -> Vec<InstalledRow> {
        plugins
            .iter()
            .map(|name| InstalledRow {
                plugin_name: name.to_string(),
                version: "1".to_string(),
            })
            .collect()
    }

    #[test]
    fn contributions_become_tabs_of_the_first_notebook() {
        let mut tree = json!({
            "type": "page",
            "children": [{ "type": "view", "children": [{ "type": "notebook", "children": [
                { "type": "page", "title": "Personal" }
            ]}]}]
        });
        let mut docs = page("/hr/employees/{id}/_tab-documents", "hr_documents");
        docs.title = "Documents".into();
        docs.component_tree = json!({
            "type": "page", "extends": "/hr/employees/{id}", "params": "employee={id}",
            "children": [{ "type": "view" }]
        });
        merge_contributions(&mut tree, vec![docs]);
        let tabs = &tree["children"][0]["children"][0]["children"];
        assert_eq!(tabs[1]["title"], "Documents");
        assert_eq!(tabs[1]["plugin"], "hr_documents");
        assert_eq!(tabs[1]["params"], "employee={id}");
    }

    #[test]
    fn contributions_make_a_notebook_when_the_page_has_none() {
        let mut tree = json!({ "type": "page", "children": [{ "type": "view" }] });
        let mut tab = page("/x/_tab-a", "p");
        tab.title = "A".into();
        merge_contributions(&mut tree, vec![tab]);
        assert_eq!(tree["children"][0]["children"][0]["type"], "notebook");
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
