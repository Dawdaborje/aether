//! The plugin catalog for developers: what can be installed, and installing it.
//!
//! `GET /api/ui/catalog` lists every plugin version loaded into the core database
//! (`aether --load-plugins`) with the organizations that have it. `POST
//! /api/ui/catalog/install` installs chosen plugins into chosen organizations, the same
//! operation as `aether --install-plugin <plugin> --org <org>`. `POST
//! /api/ui/catalog/seed` runs each plugin's `on_install` / `on_upgrade` seed commands in
//! chosen organizations (idempotent master data). `POST /api/ui/catalog/reload` re-reads
//! `plugin_paths` from the config file and loads into the catalog; `POST
//! /api/ui/catalog/upload` unpacks a zip and loads it. All are developer-only.
//! Reload/upload do not upgrade organizations.

use std::path::PathBuf;

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::Engine;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use surrealdb::types::SurrealValue;

use crate::app_dir::AppDir;
use crate::application::settings_api::require_session;
use crate::config_manager::services::peek_plugin_paths;
use crate::plugin_assets;
use crate::plugin_manager::catalog::{PluginSpec, install_plugins, upgrade_plugins};
use crate::plugin_manager::load::{self, MAX_ZIP_BYTES};
use crate::plugin_manager::revisions;
use crate::state::AppState;

/// Most plugins or organizations one install request may name.
const MAX_SELECTION: usize = 200;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/ui/catalog", get(list_catalog))
        .route("/api/ui/catalog/install", post(install))
        .route(
            "/api/ui/catalog/{name}/{version}/functions",
            get(plugin_functions),
        )
        .route(
            "/api/ui/catalog/{name}/{version}/details",
            get(plugin_details),
        )
        .route("/api/ui/catalog/seed", post(seed))
        .route("/api/ui/catalog/reload", post(reload))
        .route("/api/ui/catalog/upload", post(upload))
        .route("/api/ui/plugins/runtime", get(runtime_stats))
}

#[derive(Debug, Clone, Deserialize, Serialize, SurrealValue)]
struct AuthorRow {
    name: String,
    #[serde(default)]
    email: String,
    #[serde(default)]
    github: Option<String>,
    #[serde(default)]
    website: Option<String>,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct CatalogRow {
    name: String,
    version: String,
    label: String,
    description: Option<String>,
    long_description: Option<String>,
    workspace: Option<String>,
    kind: Option<String>,
    is_app: bool,
    is_builtin: bool,
    icon: Option<String>,
    icon_path: Option<String>,
    revision: Option<String>,
    /// ISO-8601 from `<string> date_created`.
    date_created: Option<String>,
    website: Option<String>,
    authors: Option<Vec<AuthorRow>>,
    dependencies: Option<Vec<String>>,
    app_route: Option<String>,
    app_description: Option<String>,
    commands: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct OrgRow {
    name: String,
    db_name: String,
}

#[derive(Debug, Serialize)]
struct CatalogEntry {
    name: String,
    version: String,
    label: String,
    description: Option<String>,
    long_description: Option<String>,
    category: Option<String>,
    kind: Option<String>,
    is_app: bool,
    is_builtin: bool,
    /// Lucide icon name from `[app].icon`, when the plugin is an app.
    icon: Option<String>,
    /// `/api/ui/plugins/{name}/logo` when the revision ships a logo file.
    logo_url: Option<String>,
    /// When this revision was catalogued (ISO-8601).
    date_created: Option<String>,
    /// Files new in this revision (vs the previous one; every file on the first load).
    files_added: Vec<String>,
    /// Files whose content hash changed vs the previous revision.
    files_changed: Vec<String>,
    /// Files removed vs the previous revision.
    files_removed: Vec<String>,
    website: Option<String>,
    authors: Vec<AuthorRow>,
    dependencies: Vec<String>,
    /// Launcher route from `[app].route`, when present.
    app_route: Option<String>,
    app_description: Option<String>,
    /// Database names of the organizations that have this exact version.
    installed_in: Vec<String>,
    /// Commands marked `on_install` or `on_upgrade` (master data the Desk can re-run).
    seed_commands: Vec<String>,
}

fn seed_command_names(commands: Option<&[serde_json::Value]>) -> Vec<String> {
    let Some(commands) = commands else {
        return Vec::new();
    };
    commands
        .iter()
        .filter_map(|value| {
            let def: crate::plugin_manager::models::plugin_def::PluginCommandDef =
                serde_json::from_value(value.clone()).ok()?;
            if def.on_install || def.on_upgrade {
                Some(def.name)
            } else {
                None
            }
        })
        .collect()
}

fn failure(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn database_error(context: &str, error: impl std::fmt::Display) -> Response {
    log::error!("catalog {context}: {error}");
    failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
}

async fn list_catalog(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err((status, body)) = require_session(&state, &headers).await {
        return (status, body).into_response();
    }
    match build_catalog(&state).await {
        Ok(entries) => Json(json!({ "plugins": entries })).into_response(),
        Err(error) => database_error("listing", error),
    }
}

async fn build_catalog(state: &AppState) -> Result<Vec<CatalogEntry>, surrealdb::Error> {
    let core = state.core().await?;
    let mut response = core
        .query(
            "SELECT name, version, label, description, long_description, workspace, kind, \
             is_builtin, app != NONE AS is_app, app.icon AS icon, app.route AS app_route, \
             app.description AS app_description, icon_path, revision, website, authors, \
             dependencies, commands, <string> date_created AS date_created \
             FROM plugins WHERE is_active = true ORDER BY label, name, date_created DESC;",
        )
        .query("SELECT name, db_name FROM organizations;")
        .await?
        .check()?;
    let rows: Vec<CatalogRow> = response.take(0)?;
    let organizations: Vec<OrgRow> = response.take(1)?;

    let layout = AppDir::new(&state.config.app_dir);
    let mut entries: Vec<CatalogEntry> = Vec::with_capacity(rows.len());
    for row in rows {
        let (logo_url, files_added, files_changed, files_removed) =
            revision_details(&layout, &row).await;
        entries.push(CatalogEntry {
            name: row.name,
            version: row.version,
            label: row.label,
            description: row.description,
            long_description: row.long_description,
            category: row.workspace,
            kind: row.kind,
            is_app: row.is_app,
            is_builtin: row.is_builtin,
            icon: row.icon,
            logo_url,
            date_created: row.date_created,
            files_added,
            files_changed,
            files_removed,
            website: row.website,
            authors: row.authors.unwrap_or_default(),
            dependencies: row.dependencies.unwrap_or_default(),
            app_route: row.app_route,
            app_description: row.app_description,
            installed_in: Vec::new(),
            seed_commands: seed_command_names(row.commands.as_deref()),
        });
    }

    for organization in organizations {
        let org_db = state.org(&organization.db_name).await?;
        let installed = crate::plugin_manager::installed::list_cached(
            &state.cache,
            &organization.db_name,
            &org_db,
        )
        .await?;
        for entry in &mut entries {
            if installed
                .iter()
                .any(|row| row.plugin_name == entry.name && row.version == entry.version)
            {
                entry.installed_in.push(organization.db_name.clone());
            }
        }
    }
    Ok(entries)
}

/// Logo URL plus the file-level diff for this revision vs its previous one.
async fn revision_details(
    layout: &AppDir,
    row: &CatalogRow,
) -> (Option<String>, Vec<String>, Vec<String>, Vec<String>) {
    let folder = row.revision.as_deref().unwrap_or(row.version.as_str());
    let index = match revisions::read_index(layout, &row.name, folder).await {
        Ok(Some(index)) => index,
        Ok(None) => return (None, Vec::new(), Vec::new(), Vec::new()),
        Err(error) => {
            log::warn!(
                "catalog revision index `{}@{}`: {error}",
                row.name,
                row.version
            );
            return (None, Vec::new(), Vec::new(), Vec::new());
        }
    };
    let logo_url = plugin_assets::logo_url(&row.name, &index, row.icon_path.as_deref());
    let previous = match index.previous.as_deref() {
        Some(previous_folder) => {
            match revisions::read_index(layout, &row.name, previous_folder).await {
                Ok(index) => index,
                Err(error) => {
                    log::warn!(
                        "catalog previous revision `{}` ← `{previous_folder}`: {error}",
                        row.name
                    );
                    None
                }
            }
        }
        None => None,
    };
    let diff = revisions::diff_revision(&index, previous.as_ref());
    (logo_url, diff.added, diff.changed, diff.removed)
}

#[derive(Debug, Deserialize, SurrealValue)]
struct FunctionsRow {
    artifact_path: Option<String>,
    #[serde(default)]
    public_functions: Vec<String>,
    #[serde(default)]
    commands: Vec<serde_json::Value>,
    #[serde(default)]
    schedules: Vec<serde_json::Value>,
    #[serde(default)]
    event_listeners: Vec<serde_json::Value>,
}

/// One function of a plugin and everything the manifest says about it.
#[derive(Debug, Default, Serialize)]
struct FunctionInfo {
    name: String,
    /// The code defines it (false: the manifest names it but the module does not export it).
    exported: bool,
    /// Anonymous visitors may call it (`public_functions`).
    public: bool,
    /// `rule_var_*`: answers a variable in an access rule; not meant to be called by hand.
    rule_variable: bool,
    commands: Vec<serde_json::Value>,
    schedules: Vec<serde_json::Value>,
    /// Events whose delivery runs it.
    listens_to: Vec<String>,
}

fn function_entry<'a>(
    list: &'a mut Vec<FunctionInfo>,
    name: &str,
    exported: bool,
) -> &'a mut FunctionInfo {
    if let Some(at) = list.iter().position(|f| f.name == name) {
        return &mut list[at];
    }
    list.push(FunctionInfo {
        name: name.to_string(),
        exported,
        rule_variable: name.starts_with("rule_var_"),
        ..FunctionInfo::default()
    });
    let last = list.len() - 1;
    &mut list[last]
}

/// `GET /api/ui/catalog/{name}/{version}/functions`: what one catalogued version of a
/// plugin can be called with, read from its code and joined with its manifest.
#[derive(Debug, Deserialize, SurrealValue)]
struct DetailsRow {
    revision: Option<String>,
}

/// `GET /api/ui/catalog/{name}/{version}/details`: what a catalogued version asks for and
/// offers, read from its stored `plugin.toml` and its loaded pages, models and components, so
/// an administrator can see what installing it grants before they do (developers only).
async fn plugin_details(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path((name, version)): axum::extract::Path<(String, String)>,
) -> Response {
    if let Err((status, body)) = require_session(&state, &headers).await {
        return (status, body).into_response();
    }
    let core = match state.core().await {
        Ok(core) => core,
        Err(error) => return database_error("details", error),
    };
    let rows: Vec<DetailsRow> = match core
        .query("SELECT revision FROM plugins WHERE name = $name AND version = $version LIMIT 1;")
        .bind(("name", name.clone()))
        .bind(("version", version.clone()))
        .await
        .and_then(|r| r.check())
        .and_then(|mut r| r.take(0))
    {
        Ok(rows) => rows,
        Err(error) => return database_error("details", error),
    };
    let Some(row) = rows.into_iter().next() else {
        return failure(
            StatusCode::NOT_FOUND,
            "that plugin version is not in the catalog",
        );
    };

    // The manifest, from the revision's stored `plugin.toml`.
    let layout = AppDir::new(&state.config.app_dir);
    let folder = row.revision.as_deref().unwrap_or(version.as_str());
    let mut manifest = None;
    if let Ok(Some(index)) = revisions::read_index(&layout, &name, folder).await
        && let Some(entry) = index.files.get("plugin.toml")
    {
        let path = layout
            .root()
            .join(revisions::relative_path(&name, entry, "plugin.toml"));
        if let Ok(text) = tokio::fs::read_to_string(&path).await {
            manifest = crate::plugin_manager::models::plugin_def::PluginManifest::parse(&text).ok();
        }
    }

    let pages = crate::plugin_manager::surface::pages_cached(&state.cache, &core, &name, &version)
        .await
        .unwrap_or_default();
    let models =
        crate::plugin_manager::surface::models_cached(&state.cache, &core, &name, &version)
            .await
            .unwrap_or_default();
    let components =
        crate::plugin_manager::surface::components_cached(&state.cache, &core, &name, &version)
            .await
            .unwrap_or_default();

    let access = |list: &[crate::plugin_manager::models::plugin_def::AccessModelDef]| {
        list.iter()
            .map(|model| json!({ "name": model.name, "permissions": model.permissions }))
            .collect::<Vec<_>>()
    };
    let (definition, extra) = match &manifest {
        Some(manifest) => {
            let plugin = &manifest.plugin;
            (
                json!({
                    "api_version": plugin.api_version(),
                    "capabilities": plugin.capabilities,
                    "access_models": access(&plugin.access_models),
                    "public_capabilities": plugin.public_capabilities,
                    "public_access_models": access(&plugin.public_access_models),
                    "public_functions": plugin.public_functions,
                    "http_hosts": plugin.http_hosts,
                    "bridges": plugin.bridges,
                    "i18n_default": plugin.i18n.as_ref().map(|i18n| i18n.default.clone()),
                    "code": plugin.code_file(),
                }),
                json!({
                    "roles": manifest.roles.iter().map(|role| json!({
                        "name": role.name, "label": role.label, "description": role.description,
                    })).collect::<Vec<_>>(),
                    "events": manifest.events.iter().map(|event| json!({
                        "name": event.name, "direction": event.direction,
                    })).collect::<Vec<_>>(),
                    "watches": manifest.watch.iter().map(|watch| json!({
                        "name": watch.name, "path": watch.path, "function": watch.function,
                    })).collect::<Vec<_>>(),
                }),
            )
        }
        None => (Value::Null, Value::Null),
    };

    Json(json!({
        "plugin": name,
        "version": version,
        "revision": row.revision,
        "manifest_read": manifest.is_some(),
        "definition": definition,
        "roles": extra.get("roles").cloned().unwrap_or(json!([])),
        "events": extra.get("events").cloned().unwrap_or(json!([])),
        "watches": extra.get("watches").cloned().unwrap_or(json!([])),
        "pages": pages.iter().map(|page| {
            let extends = page.component_tree.get("extends").and_then(Value::as_str);
            json!({
                "route": page.route, "title": page.title, "public": page.is_public,
                "roles": page.required_roles, "extends": extends,
            })
        }).collect::<Vec<_>>(),
        "models": models.iter().map(|(model, definition)| json!({
            "name": model,
            "label": definition.get("label").cloned(),
            "fields": definition.get("fields").and_then(Value::as_array).map(Vec::len),
        })).collect::<Vec<_>>(),
        "components": components.iter().map(|c| c.full_name.clone()).collect::<Vec<_>>(),
    }))
    .into_response()
}

async fn plugin_functions(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path((name, version)): axum::extract::Path<(String, String)>,
) -> Response {
    if let Err((status, body)) = require_session(&state, &headers).await {
        return (status, body).into_response();
    }
    let core = match state.core().await {
        Ok(core) => core,
        Err(error) => return database_error("functions", error),
    };
    let mut response = match core
        .query(
            "SELECT artifact_path, public_functions, commands, schedules, event_listeners \
             FROM plugins WHERE name = $name AND version = $version LIMIT 1;",
        )
        .bind(("name", name.clone()))
        .bind(("version", version.clone()))
        .await
        .and_then(|r| r.check())
    {
        Ok(response) => response,
        Err(error) => return database_error("functions", error),
    };
    let rows: Vec<FunctionsRow> = match response.take(0) {
        Ok(rows) => rows,
        Err(error) => return database_error("functions", error),
    };
    let Some(row) = rows.into_iter().next() else {
        return failure(
            StatusCode::NOT_FOUND,
            "that plugin version is not in the catalog",
        );
    };

    // The code: the functions it defines, and what kind of plugin it is.
    let mut kind = "unknown";
    let mut defined: Option<Vec<String>> = None;
    if let Some(path) = row.artifact_path.as_deref() {
        let file = AppDir::new(&state.config.app_dir).root().join(path);
        if let Ok(bytes) = tokio::fs::read(&file).await {
            if path.ends_with(".rhai") {
                kind = "rhai";
                defined = Some(crate::plugin_manager::exports::rhai_functions(
                    &String::from_utf8_lossy(&bytes),
                ));
            } else if path.ends_with(".lua") {
                kind = "lua";
                defined = Some(crate::plugin_manager::exports::lua_functions(
                    &String::from_utf8_lossy(&bytes),
                ));
            } else if let Some(names) = crate::plugin_manager::exports::wasm_exports(&bytes) {
                kind = "wasm";
                defined = Some(names);
            }
        }
    }

    let mut functions: Vec<FunctionInfo> = Vec::new();
    for function in defined.iter().flatten() {
        function_entry(&mut functions, function, true);
    }
    // The manifest: each way a function is reachable. One the code lacks is still listed, marked.
    let exported = |functions: &[FunctionInfo], name: &str| {
        defined.is_none() || functions.iter().any(|f| f.name == name && f.exported)
    };
    for function in &row.public_functions {
        let known = exported(&functions, function);
        function_entry(&mut functions, function, known).public = true;
    }
    for command in &row.commands {
        if let Some(function) = command.get("function").and_then(|v| v.as_str()) {
            let known = exported(&functions, function);
            function_entry(&mut functions, function, known)
                .commands
                .push(json!({
                    "name": command.get("name"),
                    "help": command.get("help"),
                    "args": command.get("args"),
                    "on_install": command.get("on_install"),
                    "on_upgrade": command.get("on_upgrade"),
                }));
        }
    }
    for task in &row.schedules {
        if let Some(function) = task.get("function").and_then(|v| v.as_str()) {
            let known = exported(&functions, function);
            function_entry(&mut functions, function, known)
                .schedules
                .push(json!({
                    "name": task.get("name"),
                    "every": task.get("every"),
                    "cron": task.get("cron"),
                }));
        }
    }
    for listener in &row.event_listeners {
        if let (Some(function), Some(event)) = (
            listener.get("function").and_then(|v| v.as_str()),
            listener.get("event").and_then(|v| v.as_str()),
        ) {
            let known = exported(&functions, function);
            function_entry(&mut functions, function, known)
                .listens_to
                .push(event.to_string());
        }
    }
    // Code that could not be read tells nothing about what is missing.
    if defined.is_none() {
        for function in &mut functions {
            function.exported = true;
        }
    }
    functions.sort_by(|a, b| a.name.cmp(&b.name));
    Json(json!({
        "plugin": name,
        "version": version,
        "kind": kind,
        // Whether the code could be read, so the page can say so instead of showing nothing.
        "inspected": defined.is_some(),
        "functions": functions,
    }))
    .into_response()
}

#[derive(Debug, Deserialize)]
struct InstallBody {
    /// `name@version`, as `--install-plugin` takes them.
    plugins: Vec<String>,
    /// Organization database names.
    organizations: Vec<String>,
}

/// What happened in one organization.
#[derive(Debug, Serialize)]
struct OrgOutcome {
    organization: String,
    installed: Vec<String>,
    /// Plugins the organization already had at another version, moved to the one chosen
    /// (`name@version`).
    upgraded: Vec<String>,
    already_installed: Vec<String>,
    error: Option<String>,
}

async fn install(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<InstallBody>,
) -> Response {
    if let Err((status, body)) = require_session(&state, &headers).await {
        return (status, body).into_response();
    }
    if body.plugins.is_empty() || body.organizations.is_empty() {
        return failure(
            StatusCode::BAD_REQUEST,
            "choose at least one app and one organization",
        );
    }
    if body.plugins.len() > MAX_SELECTION || body.organizations.len() > MAX_SELECTION {
        return failure(StatusCode::BAD_REQUEST, "too many selections");
    }
    let mut specs = Vec::with_capacity(body.plugins.len());
    for raw in &body.plugins {
        match raw.parse::<PluginSpec>() {
            Ok(spec) => specs.push(spec),
            Err(error) => return failure(StatusCode::BAD_REQUEST, &error.to_string()),
        }
    }

    // Installing selects databases itself, so it gets a session of its own.
    let session = state.fresh_session();
    let mut outcomes = Vec::with_capacity(body.organizations.len());
    for organization in body.organizations {
        // A plugin the organization already has at another version is moved to the chosen one
        // (installing it again would be refused); the rest are installed.
        crate::plugin_manager::installed::invalidate(&state.cache, &organization);
        let have: std::collections::HashMap<String, String> = match state.org(&organization).await {
            Ok(org) => {
                crate::plugin_manager::installed::list_cached(&state.cache, &organization, &org)
                    .await
                    .map(|rows| {
                        rows.into_iter()
                            .map(|row| (row.plugin_name, row.version))
                            .collect()
                    })
                    .unwrap_or_default()
            }
            Err(_) => Default::default(),
        };
        let (moves, fresh): (Vec<PluginSpec>, Vec<PluginSpec>) =
            specs.iter().cloned().partition(|spec| {
                spec.version.is_some()
                    && have
                        .get(&spec.name)
                        .is_some_and(|v| Some(v) != spec.version.as_ref())
            });
        let mut upgraded: Vec<String> = Vec::new();
        // A newer version may depend on a plugin the organization does not have yet (a shared
        // component plugin, say): install those first, so the move is not refused for them.
        let mut brought: Vec<PluginSpec> = Vec::new();
        let mut brought_installed: Vec<String> = Vec::new();
        if !moves.is_empty()
            && let Ok(core) = state.core().await
        {
            for spec in &moves {
                let mut response = match core
                    .query(
                        "SELECT VALUE dependencies FROM plugins WHERE name = $name AND version = $version LIMIT 1;",
                    )
                    .bind(("name", spec.name.clone()))
                    .bind(("version", spec.version.clone().unwrap_or_default()))
                    .await
                {
                    Ok(response) => response,
                    Err(_) => continue,
                };
                let lists: Vec<Option<Vec<String>>> = response.take(0).unwrap_or_default();
                for dependency in lists.into_iter().flatten().flatten() {
                    let present = have.contains_key(&dependency)
                        || brought.iter().any(|known| known.name == dependency)
                        || fresh.iter().any(|known| known.name == dependency);
                    if !present {
                        brought.push(PluginSpec {
                            name: dependency,
                            version: None,
                        });
                    }
                }
            }
        }
        if !brought.is_empty() {
            match install_plugins(
                &session,
                &state.namespace,
                &state.core_database,
                &organization,
                &brought,
            )
            .await
            {
                Ok(report) => {
                    crate::plugin_manager::installed::invalidate(&state.cache, &organization);
                    brought_installed = report
                        .installed
                        .iter()
                        .map(|(name, version)| format!("{name}@{version}"))
                        .collect();
                }
                Err(error) => {
                    log::warn!("installing dependencies in `{organization}` failed: {error}");
                    outcomes.push(OrgOutcome {
                        organization,
                        installed: Vec::new(),
                        upgraded: Vec::new(),
                        already_installed: Vec::new(),
                        error: Some(error.to_string()),
                    });
                    continue;
                }
            }
        }
        if !moves.is_empty() {
            match upgrade_plugins(
                &session,
                &state.namespace,
                &state.core_database,
                &organization,
                &moves,
            )
            .await
            {
                Ok(report) => {
                    crate::plugin_manager::installed::invalidate(&state.cache, &organization);
                    let names: Vec<String> = report
                        .upgraded
                        .iter()
                        .map(|(name, _, _)| name.clone())
                        .collect();
                    upgraded = report
                        .upgraded
                        .iter()
                        .map(|(name, _, to)| format!("{name}@{to}"))
                        .collect();
                    if !names.is_empty()
                        && let Err(error) = crate::plugin_manager::commands::run_lifecycle(
                            &state,
                            &organization,
                            &names,
                            crate::plugin_manager::commands::LifecyclePhase::Upgrade,
                            "system:install",
                        )
                        .await
                    {
                        log::warn!("upgrade seeds in `{organization}` failed: {error}");
                    }
                }
                Err(error) => {
                    log::warn!("upgrading in organization `{organization}` failed: {error}");
                    outcomes.push(OrgOutcome {
                        organization,
                        installed: Vec::new(),
                        upgraded: Vec::new(),
                        already_installed: Vec::new(),
                        error: Some(error.to_string()),
                    });
                    continue;
                }
            }
        }
        let result = if fresh.is_empty() {
            Ok(crate::plugin_manager::catalog::InstallReport::default())
        } else {
            install_plugins(
                &session,
                &state.namespace,
                &state.core_database,
                &organization,
                &fresh,
            )
            .await
        };
        let versions = |pairs: Vec<(String, String)>| {
            pairs
                .into_iter()
                .map(|(name, version)| format!("{name}@{version}"))
                .collect::<Vec<_>>()
        };
        outcomes.push(match result {
            Ok(report) => {
                log::info!(
                    "installed {} plugin(s) in organization `{organization}` ({} already there)",
                    report.installed.len(),
                    report.already_installed.len()
                );
                crate::plugin_manager::installed::invalidate(&state.cache, &organization);
                let seed_plugins: Vec<String> = report
                    .installed
                    .iter()
                    .map(|(name, _)| name.clone())
                    .collect();
                if !seed_plugins.is_empty() {
                    match crate::plugin_manager::commands::run_lifecycle(
                        &state,
                        &organization,
                        &seed_plugins,
                        crate::plugin_manager::commands::LifecyclePhase::Install,
                        "system:install",
                    )
                    .await
                    {
                        Ok(seeds) => {
                            for seed in seeds.iter().filter(|s| !s.ok) {
                                log::warn!(
                                    "install seed {}.{} in `{organization}` failed: {}",
                                    seed.plugin,
                                    seed.command,
                                    seed.detail.as_deref().unwrap_or("unknown")
                                );
                            }
                        }
                        Err(error) => {
                            log::warn!("install seeds in `{organization}` failed: {error}");
                        }
                    }
                }
                let mut installed = brought_installed;
                installed.extend(versions(report.installed));
                if !installed.is_empty() {
                    // The kernel tells the organization's members what changed.
                    let note = crate::notifications::NewNotification::kernel(
                        crate::notifications::Level::Success,
                        format!("{} installed", installed.join(", ")),
                        None,
                    );
                    if let Err(error) = state.notify(&organization, note).await {
                        log::warn!("install notification for `{organization}`: {error}");
                    }
                }
                OrgOutcome {
                    organization,
                    installed,
                    upgraded,
                    already_installed: versions(report.already_installed),
                    error: None,
                }
            }
            Err(error) => {
                log::warn!("installing in organization `{organization}` failed: {error}");
                OrgOutcome {
                    organization,
                    installed: Vec::new(),
                    upgraded,
                    already_installed: Vec::new(),
                    error: Some(error.to_string()),
                }
            }
        });
    }
    Json(json!({ "results": outcomes })).into_response()
}

#[derive(Debug, Deserialize)]
struct SeedBody {
    /// Plugin names (no version); each must be installed in the target organization.
    plugins: Vec<String>,
    /// Organization database names.
    organizations: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SeedOutcome {
    organization: String,
    seeded: Vec<String>,
    failed: Vec<String>,
    error: Option<String>,
}

/// Re-run master-data seed commands (`on_install` / `on_upgrade`) for installed plugins.
async fn seed(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SeedBody>,
) -> Response {
    if let Err((status, body)) = require_session(&state, &headers).await {
        return (status, body).into_response();
    }
    if body.plugins.is_empty() || body.organizations.is_empty() {
        return failure(
            StatusCode::BAD_REQUEST,
            "choose at least one app and one organization",
        );
    }
    if body.plugins.len() > MAX_SELECTION || body.organizations.len() > MAX_SELECTION {
        return failure(StatusCode::BAD_REQUEST, "too many selections");
    }
    let plugins: Vec<String> = body
        .plugins
        .into_iter()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect();
    if plugins.is_empty() {
        return failure(StatusCode::BAD_REQUEST, "choose at least one app");
    }

    let mut outcomes = Vec::with_capacity(body.organizations.len());
    for organization in body.organizations {
        match crate::plugin_manager::commands::run_lifecycle(
            &state,
            &organization,
            &plugins,
            crate::plugin_manager::commands::LifecyclePhase::Seed,
            "system:desk-seed",
        )
        .await
        {
            Ok(reports) => {
                let seeded: Vec<String> = reports
                    .iter()
                    .filter(|r| r.ok)
                    .map(|r| format!("{}.{}", r.plugin, r.command))
                    .collect();
                let failed: Vec<String> = reports
                    .iter()
                    .filter(|r| !r.ok)
                    .map(|r| {
                        format!(
                            "{}.{}: {}",
                            r.plugin,
                            r.command,
                            r.detail.as_deref().unwrap_or("failed")
                        )
                    })
                    .collect();
                if seeded.is_empty() && failed.is_empty() {
                    outcomes.push(SeedOutcome {
                        organization,
                        seeded,
                        failed,
                        error: Some(
                            "none of those plugins offer seed commands in this organization".into(),
                        ),
                    });
                } else {
                    outcomes.push(SeedOutcome {
                        organization,
                        seeded,
                        failed,
                        error: None,
                    });
                }
            }
            Err(error) => {
                log::warn!("seeding in organization `{organization}` failed: {error}");
                outcomes.push(SeedOutcome {
                    organization,
                    seeded: Vec::new(),
                    failed: Vec::new(),
                    error: Some(error.to_string()),
                });
            }
        }
    }
    Json(json!({ "results": outcomes })).into_response()
}

/// `GET /api/ui/plugins/runtime`: how the compiled-plugin cache is doing, for tuning
/// `[plugin_runtime]`. Many evictions with the same plugins loading again and again mean
/// the memory budget is too small.
async fn runtime_stats(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err((status, body)) = require_session(&state, &headers).await {
        return (status, body).into_response();
    }
    Json(json!({ "runtime": state.plugin_runtime.stats().await })).into_response()
}

#[derive(Debug, Deserialize)]
struct ReloadBody {
    /// Optional override paths (package/workspace dirs or names). When omitted,
    /// re-reads `plugin_paths` from the config file on disk.
    #[serde(default)]
    paths: Option<Vec<String>>,
    /// Move every organization that has a reloaded plugin installed to its new revision
    /// (default true), as `--watch` does. Turn off to only load into the catalog.
    #[serde(default)]
    apply: Option<bool>,
}

/// One organization moved to a freshly loaded revision.
#[derive(Debug, Serialize)]
struct Applied {
    organization: String,
    plugin: String,
    version: String,
    ok: bool,
    error: Option<String>,
}

/// Move organizations that have a plugin installed to the revision just loaded for it, and drop
/// the caches that still describe the old one. Plugins that did not change are left alone.
async fn apply_loaded(state: &AppState, loaded: &[LoadedEntry]) -> Vec<Applied> {
    let mut applied = Vec::new();
    let fresh: Vec<&LoadedEntry> = loaded.iter().filter(|row| row.created).collect();
    if fresh.is_empty() {
        return applied;
    }
    let organizations: Vec<OrgRow> = async {
        let core = state.core().await?;
        let mut response = core
            .query("SELECT name, db_name FROM organizations;")
            .await?
            .check()?;
        response.take(0)
    }
    .await
    .unwrap_or_default();

    for row in &fresh {
        crate::plugin_manager::surface::invalidate(&state.cache, &row.name, &row.version);
    }
    let session = state.fresh_session();
    for organization in organizations {
        crate::plugin_manager::installed::invalidate(&state.cache, &organization.db_name);
        let have: Vec<String> = match state.org(&organization.db_name).await {
            Ok(org) => crate::plugin_manager::installed::list_cached(
                &state.cache,
                &organization.db_name,
                &org,
            )
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(|installed| installed.plugin_name)
                    .collect()
            })
            .unwrap_or_default(),
            Err(_) => continue,
        };
        // Loaded dependencies first, so a dependent never moves ahead of what it needs.
        for row in fresh.iter().filter(|row| have.contains(&row.name)) {
            let spec = PluginSpec {
                name: row.name.clone(),
                version: Some(row.version.clone()),
            };
            let result = upgrade_plugins(
                &session,
                &state.namespace,
                &state.core_database,
                &organization.db_name,
                std::slice::from_ref(&spec),
            )
            .await;
            let error = match result {
                Ok(report) => {
                    crate::plugin_manager::installed::invalidate(
                        &state.cache,
                        &organization.db_name,
                    );
                    if !report.upgraded.is_empty()
                        && let Err(error) = crate::plugin_manager::commands::run_lifecycle(
                            state,
                            &organization.db_name,
                            std::slice::from_ref(&row.name),
                            crate::plugin_manager::commands::LifecyclePhase::Upgrade,
                            "system:reload",
                        )
                        .await
                    {
                        log::warn!("upgrade seeds for `{}` failed: {error}", row.name);
                    }
                    None
                }
                Err(error) => {
                    log::warn!(
                        "moving `{}` in `{}` to {} failed: {error}",
                        row.name,
                        organization.db_name,
                        row.version
                    );
                    Some(error.to_string())
                }
            };
            applied.push(Applied {
                organization: organization.db_name.clone(),
                plugin: row.name.clone(),
                version: row.version.clone(),
                ok: error.is_none(),
                error,
            });
        }
    }
    applied
}

#[derive(Debug, Serialize)]
struct LoadedEntry {
    name: String,
    version: String,
    created: bool,
    revision: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UploadBody {
    filename: String,
    content_base64: String,
}

fn search_roots_for(state: &AppState) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }
    if let Some(config_path) = &state.config_path
        && let Some(parent) = config_path.parent()
    {
        let parent = parent.to_path_buf();
        if !roots.iter().any(|root| root == &parent) {
            roots.push(parent);
        }
    }
    if roots.is_empty() {
        roots.push(PathBuf::from("."));
    }
    roots
}

fn resolve_reload_entries(state: &AppState, body: &ReloadBody) -> Result<Vec<PathBuf>, Response> {
    if let Some(paths) = &body.paths {
        let entries: Vec<PathBuf> = paths
            .iter()
            .map(|raw| raw.trim())
            .filter(|raw| !raw.is_empty())
            .map(PathBuf::from)
            .collect();
        if entries.is_empty() {
            return Err(failure(
                StatusCode::BAD_REQUEST,
                "paths must name at least one package, workspace, or workspace name",
            ));
        }
        return Ok(entries);
    }
    if let Some(config_path) = &state.config_path {
        match peek_plugin_paths(config_path) {
            Ok(paths) if !paths.is_empty() => Ok(paths),
            Ok(_) => Err(failure(
                StatusCode::BAD_REQUEST,
                "plugin_paths in the config file is empty; add packages or workspaces, or pass paths",
            )),
            Err(error) => {
                log::warn!("catalog reload: could not re-read plugin_paths: {error}");
                Err(failure(
                    StatusCode::BAD_REQUEST,
                    &format!(
                        "could not read plugin_paths from {}: {error}",
                        config_path.display()
                    ),
                ))
            }
        }
    } else if !state.config.plugin_paths.is_empty() {
        Ok(state.config.plugin_paths.clone())
    } else {
        Err(failure(
            StatusCode::BAD_REQUEST,
            "no config file path is known and plugin_paths is empty; pass paths or restart with -c aether.toml",
        ))
    }
}

async fn run_catalog_load(
    state: &AppState,
    entries: &[PathBuf],
) -> Result<Vec<LoadedEntry>, Response> {
    let search_roots = search_roots_for(state);
    let session = state.fresh_session();
    let loaded = load::load_into_catalog(
        &session,
        &state.namespace,
        &state.core_database,
        &state.config.app_dir,
        Some(&state.cache),
        entries,
        &search_roots,
    )
    .await
    .map_err(|error| {
        log::warn!("catalog load failed: {error}");
        failure(StatusCode::BAD_REQUEST, &error.to_string())
    })?;
    Ok(loaded
        .into_iter()
        .map(|plugin| LoadedEntry {
            name: plugin.name,
            version: plugin.version,
            created: plugin.created,
            revision: plugin.revision,
        })
        .collect())
}

async fn reload(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ReloadBody>,
) -> Response {
    if let Err((status, body)) = require_session(&state, &headers).await {
        return (status, body).into_response();
    }
    let entries = match resolve_reload_entries(&state, &body) {
        Ok(entries) => entries,
        Err(response) => return response,
    };
    match run_catalog_load(&state, &entries).await {
        Ok(loaded) => {
            let created = loaded.iter().filter(|row| row.created).count();
            log::info!(
                "catalog reload: {} package(s), {} new revision(s)",
                loaded.len(),
                created
            );
            let applied = if body.apply.unwrap_or(true) {
                apply_loaded(&state, &loaded).await
            } else {
                Vec::new()
            };
            Json(json!({ "loaded": loaded, "applied": applied })).into_response()
        }
        Err(response) => response,
    }
}

async fn upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<UploadBody>,
) -> Response {
    if let Err((status, body)) = require_session(&state, &headers).await {
        return (status, body).into_response();
    }
    let filename = body.filename.trim();
    if filename.is_empty() {
        return failure(StatusCode::BAD_REQUEST, "filename is required");
    }
    if !filename.to_ascii_lowercase().ends_with(".zip") {
        return failure(StatusCode::BAD_REQUEST, "upload a .zip file");
    }
    let raw = body
        .content_base64
        .split(',')
        .next_back()
        .unwrap_or(body.content_base64.as_str())
        .trim();
    let bytes = match base64::engine::general_purpose::STANDARD.decode(raw) {
        Ok(bytes) => bytes,
        Err(_) => {
            return failure(StatusCode::BAD_REQUEST, "the file data is not valid base64");
        }
    };
    if bytes.len() > MAX_ZIP_BYTES {
        return failure(
            StatusCode::BAD_REQUEST,
            &format!("zip is larger than {} MB", MAX_ZIP_BYTES / (1024 * 1024)),
        );
    }

    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let layout = AppDir::new(&state.config.app_dir);
    let dest = match layout.ensure_plugin_upload(&stamp).await {
        Ok(path) => path,
        Err(error) => {
            log::error!("catalog upload staging: {error}");
            return failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not stage the upload",
            );
        }
    };

    let package_root = match tokio::task::spawn_blocking({
        let dest = dest.clone();
        move || load::unpack_plugin_zip(&bytes, &dest)
    })
    .await
    {
        Ok(Ok(root)) => root,
        Ok(Err(error)) => {
            log::warn!("catalog upload unpack: {error}");
            let _ = tokio::fs::remove_dir_all(&dest).await;
            return failure(StatusCode::BAD_REQUEST, &error.to_string());
        }
        Err(error) => {
            log::error!("catalog upload unpack task: {error}");
            let _ = tokio::fs::remove_dir_all(&dest).await;
            return failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not unpack the zip",
            );
        }
    };

    let entries = vec![package_root];
    match run_catalog_load(&state, &entries).await {
        Ok(loaded) => {
            let created = loaded.iter().filter(|row| row.created).count();
            log::info!(
                "catalog upload `{}`: {} package(s), {} new revision(s) under {}",
                filename,
                loaded.len(),
                created,
                dest.display()
            );
            let applied = apply_loaded(&state, &loaded).await;
            Json(json!({
                "loaded": loaded,
                "applied": applied,
                "staged": dest.display().to_string(),
            }))
            .into_response()
        }
        Err(response) => response,
    }
}
