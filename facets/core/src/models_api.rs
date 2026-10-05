//! The developer's model editor.
//!
//! * `GET  /api/ui/models`: every plugin's models, as they are in its newest version.
//! * `POST /api/ui/models/{plugin}/plan`: what saving an edit would do to organizations.
//! * `PUT  /api/ui/models/{plugin}`: save an edit.
//!
//! A save makes a new version of the plugin (see `plugin_manager::model_edit`) and, unless
//! `write_file` is off, also writes the model into the plugin's JSON file in the folder it was
//! loaded from, so the source stays the same as what the database holds. With `write_file` off
//! the change lives only in the database, and the file is left alone. `apply_to` names the
//! organizations to move to the new version right away; the rest keep the version they have.
//!
//! All developer-only.

use std::path::{Path, PathBuf};

use axum::{
    Json, Router,
    extract::{Path as UrlPath, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use surrealdb::types::SurrealValue;

use crate::app_dir::AppDir;
use crate::application::settings_api::require_session;
use crate::data_model::{ModelDef, apply::{self, Op}, sync_ids};
use crate::plugin_manager::catalog::{PluginSpec, upgrade_plugins};
use crate::plugin_manager::model_edit::{EditError, merge_models, models_of_version, publish_models, validate_all};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/ui/models", get(list_models))
        .route("/api/ui/models/{plugin}/plan", post(plan_edit))
        .route("/api/ui/models/{plugin}", axum::routing::put(save_edit))
}

fn failure(status: StatusCode, message: impl AsRef<str>) -> Response {
    (status, Json(json!({ "error": message.as_ref() }))).into_response()
}

fn database_error(context: &str, error: impl std::fmt::Display) -> Response {
    log::error!("models {context}: {error}");
    failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
}

fn edit_failure(error: EditError) -> Response {
    match &error {
        EditError::PluginNotFound(_) | EditError::VersionNotFound { .. } => failure(StatusCode::NOT_FOUND, error.to_string()),
        EditError::Invalid(_) | EditError::Models(_) => failure(StatusCode::BAD_REQUEST, error.to_string()),
        EditError::FilesMissing(_) => failure(StatusCode::CONFLICT, error.to_string()),
        other => database_error("edit", other),
    }
}

#[derive(Debug, Deserialize, SurrealValue)]
struct PluginRow {
    name: String,
    label: String,
    version: String,
    source_path: Option<String>,
}

/// Each active plugin at its newest version.
async fn latest_plugins(state: &AppState) -> Result<Vec<PluginRow>, surrealdb::Error> {
    let core = state.core().await?;
    let mut response = core
        .query("SELECT name, label, version, source_path, date_created FROM plugins WHERE is_active = true ORDER BY date_created DESC;")
        .await?
        .check()?;
    let rows: Vec<PluginRow> = response.take(0)?;
    let mut seen = std::collections::BTreeSet::new();
    Ok(rows.into_iter().filter(|row| seen.insert(row.name.clone())).collect())
}

/// The folder the plugin was loaded from, when it is still there.
async fn source_folder(source_path: Option<&str>) -> Option<PathBuf> {
    let path = PathBuf::from(source_path?);
    tokio::fs::metadata(path.join("plugin.toml")).await.ok().map(|_| path)
}

async fn list_models(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err((status, body)) = require_session(&state, &headers).await {
        return (status, body).into_response();
    }
    let plugins = match latest_plugins(&state).await {
        Ok(plugins) => plugins,
        Err(error) => return database_error("listing", error),
    };
    let core = match state.core().await {
        Ok(core) => core,
        Err(error) => return database_error("core", error),
    };
    let mut out = Vec::new();
    for plugin in plugins {
        let models = match models_of_version(&core, &plugin.name, &plugin.version).await {
            Ok(models) => models,
            Err(error) => return edit_failure(error),
        };
        out.push(json!({
            "name": plugin.name,
            "label": plugin.label,
            "version": plugin.version,
            "models": models,
            "file_available": source_folder(plugin.source_path.as_deref()).await.is_some(),
        }));
    }
    Json(json!({ "plugins": out })).into_response()
}

#[derive(Debug, Deserialize)]
struct PlanBody {
    model: ModelDef,
    /// The version to edit; the newest when left out.
    version: Option<String>,
    #[serde(default)]
    organizations: Vec<String>,
}

fn describe(op: &Op) -> Value {
    match op {
        Op::CreateTable { .. } => json!({ "kind": "create_table" }),
        Op::DefineField { comment, surql_type, .. } => json!({ "kind": "define_field", "field": comment, "type": surql_type }),
        Op::DefineIndex { field, unique, .. } => json!({ "kind": "define_index", "field": field, "unique": unique }),
        Op::RemoveIndex { field, .. } => json!({ "kind": "remove_index", "field": field }),
        Op::DefineEdge { edge, .. } => json!({ "kind": "define_edge", "edge": edge }),
        Op::RebuildEdges { edge, .. } => json!({ "kind": "rebuild_edges", "edge": edge }),
        Op::DefineCompositeIndex { index, fields, unique, .. } => {
            json!({ "kind": "define_composite_index", "index": index, "fields": fields, "unique": unique })
        }
        Op::RemoveCompositeIndex { index, .. } => json!({ "kind": "remove_composite_index", "index": index }),
        Op::Backfill { field, .. } => json!({ "kind": "backfill", "field": field }),
    }
}

async fn plan_edit(
    State(state): State<AppState>,
    headers: HeaderMap,
    UrlPath(plugin): UrlPath<String>,
    Json(body): Json<PlanBody>,
) -> Response {
    if let Err((status, response)) = require_session(&state, &headers).await {
        return (status, response).into_response();
    }
    let mut model = body.model;
    sync_ids(&mut model);
    let (merged, base) = match merged_models(&state, &plugin, body.version.as_deref(), &model).await {
        Ok(found) => found,
        Err(response) => return response,
    };

    // What it would do in each organization, on a session that can move between databases.
    let session = state.fresh_session();
    let mut results = Vec::new();
    for organization in body.organizations {
        if let Err(error) = session.use_ns(&state.namespace).use_db(&organization).await {
            return database_error("organization", error);
        }
        let plans = match apply::plan_models(&session, &plugin, &merged).await {
            Ok(plans) => plans,
            Err(error) => return database_error("plan", error),
        };
        let mine = plans.iter().find(|(plan, _)| plan.model == model.name);
        results.push(match mine {
            Some((plan, _)) => json!({
                "organization": organization,
                "changes": plan.ops.iter().map(describe).collect::<Vec<_>>(),
                "blockers": plan.blockers,
                "notes": plan.notes,
            }),
            None => json!({ "organization": organization, "changes": [], "blockers": [], "notes": [] }),
        });
    }
    Json(json!({ "model": model, "based_on": base, "organizations": results })).into_response()
}

/// The plugin's models with `model` merged in, checked; and the version they are based on.
async fn merged_models(
    state: &AppState,
    plugin: &str,
    version: Option<&str>,
    model: &ModelDef,
) -> Result<(Vec<ModelDef>, String), Response> {
    let plugins = latest_plugins(state).await.map_err(|error| database_error("plugins", error))?;
    let base = match version {
        Some(version) => version.to_string(),
        None => plugins
            .iter()
            .find(|row| row.name == plugin)
            .map(|row| row.version.clone())
            .ok_or_else(|| failure(StatusCode::NOT_FOUND, format!("plugin `{plugin}` is not in the catalog")))?,
    };
    let core = state.core().await.map_err(|error| database_error("core", error))?;
    let existing = models_of_version(&core, plugin, &base).await.map_err(edit_failure)?;
    let merged = merge_models(&existing, std::slice::from_ref(model)).map_err(edit_failure)?;
    validate_all(&merged).map_err(edit_failure)?;
    Ok((merged, base))
}

#[derive(Debug, Deserialize)]
struct SaveBody {
    model: ModelDef,
    version: Option<String>,
    /// Also write the model into the plugin's JSON file. On unless turned off.
    #[serde(default = "yes")]
    write_file: bool,
    /// Organizations to move to the new version now.
    #[serde(default)]
    apply_to: Vec<String>,
}

fn yes() -> bool {
    true
}

async fn save_edit(
    State(state): State<AppState>,
    headers: HeaderMap,
    UrlPath(plugin): UrlPath<String>,
    Json(body): Json<SaveBody>,
) -> Response {
    if let Err((status, response)) = require_session(&state, &headers).await {
        return (status, response).into_response();
    }
    let mut model = body.model;
    sync_ids(&mut model);
    let (merged, base) = match merged_models(&state, &plugin, body.version.as_deref(), &model).await {
        Ok(found) => found,
        Err(response) => return response,
    };

    // The file first: asking for it when it cannot be written is a mistake to report, before
    // anything is saved.
    let folder = if body.write_file {
        let plugins = match latest_plugins(&state).await {
            Ok(plugins) => plugins,
            Err(error) => return database_error("plugins", error),
        };
        let source = plugins.iter().find(|row| row.name == plugin).and_then(|row| row.source_path.clone());
        match source_folder(source.as_deref()).await {
            Some(folder) => Some(folder),
            None => {
                return (
                    StatusCode::CONFLICT,
                    Json(json!({
                        "error": "the plugin's source folder is not on this machine, so the model file cannot be written; save to the database only",
                        "file_available": false,
                    })),
                )
                    .into_response();
            }
        }
    } else {
        None
    };

    // Organizations that would be blocked stop the save, before anything is changed.
    let session = state.fresh_session();
    for organization in &body.apply_to {
        if let Err(error) = session.use_ns(&state.namespace).use_db(organization).await {
            return database_error("organization", error);
        }
        match apply::plan_models(&session, &plugin, &merged).await {
            Ok(plans) => {
                if let Some((blocked, problems)) = apply::blockers(&plans).into_iter().next() {
                    return (
                        StatusCode::CONFLICT,
                        Json(json!({
                            "error": format!("`{blocked}` cannot be applied to `{organization}`: {}", problems.join("; ")),
                            "organization": organization,
                            "blockers": problems,
                        })),
                    )
                        .into_response();
                }
            }
            Err(error) => return database_error("plan", error),
        }
    }

    let core = match state.core().await {
        Ok(core) => core,
        Err(error) => return database_error("core", error),
    };
    let layout = AppDir::new(&state.config.app_dir);
    let published = match publish_models(&core, &layout, &plugin, Some(&base), std::slice::from_ref(&model)).await {
        Ok(published) => published,
        Err(error) => return edit_failure(error),
    };
    log::info!("model `{}` of plugin `{plugin}` saved as {}", model.name, published.version);

    let file = match &folder {
        Some(folder) => write_model_file(folder, &model).await,
        None => json!({ "written": false, "reason": "write_file is off" }),
    };

    let mut applied = Vec::new();
    for organization in &body.apply_to {
        let spec = PluginSpec { name: plugin.clone(), version: Some(published.version.clone()) };
        let result = upgrade_plugins(&session, &state.namespace, &state.core_database, organization, &[spec]).await;
        applied.push(match result {
            Ok(_) => json!({ "organization": organization, "ok": true }),
            Err(error) => {
                log::warn!("upgrading `{organization}` to {} failed: {error}", published.version);
                json!({ "organization": organization, "ok": false, "error": error.to_string() })
            }
        });
    }

    Json(json!({
        "version": published.version,
        "revision": published.revision,
        "from": published.from,
        "model": model,
        "file": file,
        "applied": applied,
        // A model the plugin has no grant on cannot be used by its code or pages yet.
        "note": "Plugin code and pages reach a model only through `access_models` in plugin.toml.",
    }))
    .into_response()
}

/// Write `models/<name>.json` in the plugin's source folder.
async fn write_model_file(folder: &Path, model: &ModelDef) -> Value {
    let directory = folder.join("models");
    let path = directory.join(format!("{}.json", model.name));
    let result = async {
        tokio::fs::create_dir_all(&directory).await?;
        let mut text = serde_json::to_string_pretty(model).map_err(std::io::Error::other)?;
        text.push('\n');
        tokio::fs::write(&path, text).await
    }
    .await;
    match result {
        Ok(()) => json!({ "written": true, "path": path.to_string_lossy() }),
        Err(error) => {
            log::warn!("could not write {}: {error}", path.display());
            json!({ "written": false, "reason": error.to_string() })
        }
    }
}
