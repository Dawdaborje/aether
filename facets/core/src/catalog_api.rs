//! The plugin catalog for developers: what can be installed, and installing it.
//!
//! `GET /api/ui/catalog` lists every plugin version loaded into the core database
//! (`aether --load-plugin`) with the organizations that have it. `POST
//! /api/ui/catalog/install` installs chosen plugins into chosen organizations, the same
//! operation as `aether --install-plugin <plugin> --org <org>`. Both are developer-only.

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use surrealdb::types::SurrealValue;

use crate::application::settings_api::require_session;
use crate::plugin_manager::catalog::{PluginSpec, install_plugins};
use crate::state::AppState;

/// Most plugins or organizations one install request may name.
const MAX_SELECTION: usize = 200;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/ui/catalog", get(list_catalog))
        .route("/api/ui/catalog/install", post(install))
}

#[derive(Debug, Deserialize, SurrealValue)]
struct CatalogRow {
    name: String,
    version: String,
    label: String,
    description: Option<String>,
    workspace: Option<String>,
    kind: Option<String>,
    is_app: bool,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct OrgRow {
    name: String,
    db_name: String,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct InstalledRow {
    plugin_name: String,
    version: String,
}

#[derive(Debug, Serialize)]
struct CatalogEntry {
    name: String,
    version: String,
    label: String,
    description: Option<String>,
    category: Option<String>,
    kind: Option<String>,
    is_app: bool,
    /// Database names of the organizations that have this exact version.
    installed_in: Vec<String>,
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
            "SELECT name, version, label, description, workspace, kind, app != NONE AS is_app \
             FROM plugins WHERE is_active = true ORDER BY name, version;",
        )
        .query("SELECT name, db_name FROM organizations;")
        .await?
        .check()?;
    let rows: Vec<CatalogRow> = response.take(0)?;
    let organizations: Vec<OrgRow> = response.take(1)?;

    let mut entries: Vec<CatalogEntry> = rows
        .into_iter()
        .map(|row| CatalogEntry {
            name: row.name,
            version: row.version,
            label: row.label,
            description: row.description,
            category: row.workspace,
            kind: row.kind,
            is_app: row.is_app,
            installed_in: Vec::new(),
        })
        .collect();

    for organization in organizations {
        let org_db = state.org(&organization.db_name).await?;
        let mut response = org_db
            .query("SELECT plugin_name, version FROM installed_plugins;")
            .await?
            .check()?;
        let installed: Vec<InstalledRow> = response.take(0)?;
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
        return failure(StatusCode::BAD_REQUEST, "choose at least one app and one organization");
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
        let result = install_plugins(
            &session,
            &state.namespace,
            &state.core_database,
            &organization,
            &specs,
        )
        .await;
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
                OrgOutcome {
                    organization,
                    installed: versions(report.installed),
                    already_installed: versions(report.already_installed),
                    error: None,
                }
            }
            Err(error) => {
                log::warn!("installing in organization `{organization}` failed: {error}");
                OrgOutcome {
                    organization,
                    installed: Vec::new(),
                    already_installed: Vec::new(),
                    error: Some(error.to_string()),
                }
            }
        });
    }
    Json(json!({ "results": outcomes })).into_response()
}
