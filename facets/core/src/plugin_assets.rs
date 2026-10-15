//! Serve static files a loaded plugin ships under `assets/` (and its `icon_path`).
//!
//! - `GET /api/ui/plugins/{plugin}/assets/{*path}` — a file from the latest revision
//! - `GET /api/ui/plugins/{plugin}/logo` — `icon_path` or a conventional `assets/logo.*` /
//!   `assets/icon.*` file
//!
//! Only paths under `assets/` or the plugin's declared `icon_path` are served. Logged-in
//! users may fetch them (the desk and the apps launcher use logos).

use std::path::{Component, Path, PathBuf};

use axum::{
    Json, Router,
    body::Body,
    extract::{Path as AxumPath, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use serde_json::json;
use surrealdb::types::SurrealValue;

use crate::app_dir::AppDir;
use crate::application::settings_api::require_session;
use crate::plugin_manager::revisions::{self, FileEntry, RevisionIndex, relative_path};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/ui/plugins/{plugin}/logo", get(serve_logo))
        .route("/api/ui/plugins/{plugin}/assets/{*path}", get(serve_asset))
        .route(
            "/api/ui/workspaces/{workspace}/logo",
            get(serve_workspace_logo),
        )
}

#[derive(Debug, Deserialize, SurrealValue)]
struct WorkspaceLogoRow {
    name: String,
    logo_file: Option<String>,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct PluginFilesRow {
    name: String,
    revision: Option<String>,
    version: String,
    icon_path: Option<String>,
}

fn failure(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn safe_relative(path: &str) -> Option<PathBuf> {
    let trimmed = path.trim_start_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let candidate = Path::new(trimmed);
    if candidate
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return None;
    }
    Some(candidate.to_path_buf())
}

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "json" => "application/json",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "txt" | "md" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Whether `logical` may be served for this plugin.
fn allowed_asset(logical: &str, icon_path: Option<&str>) -> bool {
    let logical = logical.trim_start_matches('/');
    if logical.starts_with("assets/") && logical != "assets/" {
        return true;
    }
    icon_path.is_some_and(|icon| icon.trim_start_matches('/') == logical)
}

/// Preferred logo path for a plugin, if any of the usual files are in its revision.
pub fn logo_logical(
    files: &std::collections::BTreeMap<String, FileEntry>,
    icon_path: Option<&str>,
) -> Option<String> {
    const CANDIDATES: &[&str] = &[
        "assets/logo.svg",
        "assets/logo.png",
        "assets/logo.webp",
        "assets/icon.svg",
        "assets/icon.png",
        "assets/icon.webp",
    ];
    if let Some(icon) = icon_path.map(|path| path.trim_start_matches('/'))
        && !icon.is_empty()
        && files.contains_key(icon)
    {
        return Some(icon.to_string());
    }
    CANDIDATES
        .iter()
        .find(|path| files.contains_key(**path))
        .map(|path| (*path).to_string())
}

/// Public URL for a plugin's logo when the revision ships one.
pub fn logo_url(plugin: &str, files: &RevisionIndex, icon_path: Option<&str>) -> Option<String> {
    logo_logical(&files.files, icon_path).map(|_| format!("/api/ui/plugins/{plugin}/logo"))
}

async fn lookup_plugin(state: &AppState, plugin: &str) -> Result<PluginFilesRow, Response> {
    let core = state.core().await.map_err(|error| {
        log::error!("plugin asset: core db: {error}");
        failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
    })?;
    let mut response = core
        .query(
            "SELECT name, revision, version, icon_path, date_created FROM plugins \
             WHERE name = $name AND is_active = true ORDER BY date_created DESC LIMIT 1;",
        )
        .bind(("name", plugin.to_string()))
        .await
        .and_then(|response| response.check())
        .map_err(|error| {
            log::error!("plugin asset: lookup `{plugin}`: {error}");
            failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        })?;
    let rows: Vec<PluginFilesRow> = response.take(0).map_err(|error| {
        log::error!("plugin asset: decode `{plugin}`: {error}");
        failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
    })?;
    rows.into_iter()
        .next()
        .ok_or_else(|| failure(StatusCode::NOT_FOUND, "plugin not found"))
}

async fn read_plugin_index(
    state: &AppState,
    row: &PluginFilesRow,
) -> Result<RevisionIndex, Response> {
    let folder = row.revision.as_deref().unwrap_or(row.version.as_str());
    let layout = AppDir::new(&state.config.app_dir);
    match revisions::read_index(&layout, &row.name, folder).await {
        Ok(Some(index)) => Ok(index),
        Ok(None) => Err(failure(StatusCode::NOT_FOUND, "asset not found")),
        Err(error) => {
            log::error!("plugin asset: index `{}`: {error}", row.name);
            Err(failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not read plugin files",
            ))
        }
    }
}

async fn serve_file(state: &AppState, row: &PluginFilesRow, logical: &str) -> Response {
    if !allowed_asset(logical, row.icon_path.as_deref()) {
        return failure(StatusCode::NOT_FOUND, "asset not found");
    }
    let index = match read_plugin_index(state, row).await {
        Ok(index) => index,
        Err(response) => return response,
    };
    let Some(entry) = index.files.get(logical) else {
        return failure(StatusCode::NOT_FOUND, "asset not found");
    };
    let layout = AppDir::new(&state.config.app_dir);
    let relative = relative_path(&row.name, entry, logical);
    let absolute = layout.root().join(&relative);
    let bytes = match tokio::fs::read(&absolute).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return failure(StatusCode::NOT_FOUND, "asset not found");
        }
        Err(error) => {
            log::error!("plugin asset: read {}: {error}", absolute.display());
            return failure(StatusCode::INTERNAL_SERVER_ERROR, "could not read asset");
        }
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type(Path::new(logical)))
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(bytes))
        .unwrap_or_else(|_| failure(StatusCode::INTERNAL_SERVER_ERROR, "response error"))
}

async fn serve_asset(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((plugin, path)): AxumPath<(String, String)>,
) -> Response {
    if let Err((status, body)) = require_session(&state, &headers).await {
        return (status, body).into_response();
    }
    let Some(logical) = safe_relative(&path) else {
        return failure(StatusCode::BAD_REQUEST, "invalid asset path");
    };
    // The route already names the folder: `/assets/brand.svg` is `assets/brand.svg`.
    let logical = format!("assets/{}", logical.to_string_lossy().replace('\\', "/"));
    let row = match lookup_plugin(&state, &plugin).await {
        Ok(row) => row,
        Err(response) => return response,
    };
    serve_file(&state, &row, &logical).await
}

async fn serve_logo(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(plugin): AxumPath<String>,
) -> Response {
    if let Err((status, body)) = require_session(&state, &headers).await {
        return (status, body).into_response();
    }
    let row = match lookup_plugin(&state, &plugin).await {
        Ok(row) => row,
        Err(response) => return response,
    };
    let index = match read_plugin_index(&state, &row).await {
        Ok(index) => index,
        Err(response) => return response,
    };
    let Some(logical) = logo_logical(&index.files, row.icon_path.as_deref()) else {
        return failure(StatusCode::NOT_FOUND, "logo not found");
    };
    serve_file(&state, &row, &logical).await
}

async fn serve_workspace_logo(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(workspace): AxumPath<String>,
) -> Response {
    if let Err((status, body)) = require_session(&state, &headers).await {
        return (status, body).into_response();
    }
    let core = match state.core().await {
        Ok(db) => db,
        Err(error) => {
            log::error!("workspace logo: core db: {error}");
            return failure(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    };
    let mut response = match core
        .query("SELECT name, logo_file FROM plugin_workspaces WHERE name = $name LIMIT 1;")
        .bind(("name", workspace.clone()))
        .await
        .and_then(|response| response.check())
    {
        Ok(response) => response,
        Err(error) => {
            log::error!("workspace logo: lookup `{workspace}`: {error}");
            return failure(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    };
    let rows: Vec<WorkspaceLogoRow> = match response.take(0) {
        Ok(rows) => rows,
        Err(error) => {
            log::error!("workspace logo: decode `{workspace}`: {error}");
            return failure(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    };
    let Some(row) = rows.into_iter().next() else {
        return failure(StatusCode::NOT_FOUND, "workspace not found");
    };
    let layout = AppDir::new(&state.config.app_dir);
    let Some(path) = crate::plugin_manager::workspaces::staged_logo_path(
        &layout,
        &row.name,
        row.logo_file.as_deref(),
    )
    .await
    else {
        return failure(StatusCode::NOT_FOUND, "logo not found");
    };
    let bytes = match tokio::fs::read(&path).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return failure(StatusCode::NOT_FOUND, "logo not found");
        }
        Err(error) => {
            log::error!("workspace logo: read {}: {error}", path.display());
            return failure(StatusCode::INTERNAL_SERVER_ERROR, "could not read asset");
        }
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type(&path))
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(bytes))
        .unwrap_or_else(|_| failure(StatusCode::INTERNAL_SERVER_ERROR, "response error"))
}
