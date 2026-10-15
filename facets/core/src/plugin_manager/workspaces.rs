//! Register plugin workspaces (`workspace.toml`) for the Apps launcher groups.

use std::path::{Path, PathBuf};

use surrealdb::{Surreal, engine::remote::ws::Client};
use thiserror::Error;

use crate::app_dir::{AppDir, AppDirError};

use super::load_paths::WorkspaceSource;

#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error(transparent)]
    AppDir(#[from] AppDirError),
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("database: {0}")]
    Database(String),
}

/// Whether `group_icon` names a file under the workspace (vs a Lucide icon id).
pub fn is_asset_icon(icon: &str) -> bool {
    let trimmed = icon.trim();
    trimmed.contains('/') || trimmed.contains('\\') || trimmed.starts_with('.')
}

fn logo_file_name(source: &Path) -> String {
    let ext = source
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("bin")
        .to_ascii_lowercase();
    match ext.as_str() {
        "svg" | "png" | "webp" | "jpg" | "jpeg" | "gif" | "ico" => format!("logo.{ext}"),
        _ => "logo.bin".into(),
    }
}

/// Copy a workspace group logo into `app_dir/workspaces/<name>/` when present.
async fn stage_logo(
    layout: &AppDir,
    workspace: &WorkspaceSource,
) -> Result<Option<String>, WorkspaceError> {
    let Some(icon) = workspace
        .group_icon
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    else {
        return Ok(None);
    };
    if !is_asset_icon(icon) {
        return Ok(None);
    }
    let source = workspace.dir.join(icon);
    if !tokio::fs::try_exists(&source).await.unwrap_or(false) {
        log::warn!(
            "workspace `{}`: group_icon `{}` not found at {}",
            workspace.name,
            icon,
            source.display()
        );
        return Ok(None);
    }
    let dest_dir = layout.workspace_dir(&workspace.name)?;
    tokio::fs::create_dir_all(&dest_dir)
        .await
        .map_err(|source| WorkspaceError::Io {
            path: dest_dir.clone(),
            source,
        })?;
    let file_name = logo_file_name(&source);
    let dest = dest_dir.join(&file_name);
    tokio::fs::copy(&source, &dest)
        .await
        .map_err(|source| WorkspaceError::Io {
            path: dest.clone(),
            source,
        })?;
    Ok(Some(file_name))
}

/// Upsert workspace rows and stage logos for the Apps launcher.
pub async fn register_workspaces(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    app_dir: &Path,
    workspaces: &[WorkspaceSource],
) -> Result<(), WorkspaceError> {
    if workspaces.is_empty() {
        return Ok(());
    }
    db.use_ns(namespace)
        .use_db(core_database)
        .await
        .map_err(|error| WorkspaceError::Database(error.to_string()))?;

    let layout = AppDir::new(app_dir);
    layout.ensure().await?;

    for workspace in workspaces {
        let logo_file = stage_logo(&layout, workspace).await?;
        let icon_for_db = workspace.group_icon.as_ref().and_then(|icon| {
            let trimmed = icon.trim();
            if trimmed.is_empty() {
                None
            } else if is_asset_icon(trimmed) {
                // Asset path is staged as logo_file; keep the path for debugging.
                Some(trimmed.to_string())
            } else {
                Some(trimmed.to_string())
            }
        });
        let launcher = workspace
            .launcher
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty());
        let launcher = match launcher {
            Some("flat") | Some("folder") => launcher.map(str::to_string),
            Some(other) => {
                log::warn!(
                    "workspace `{}`: unknown launcher `{other}` (use flat or folder); defaulting to folder",
                    workspace.name
                );
                Some("folder".into())
            }
            None => None,
        };
        // SET keeps `date_created` (schema default on insert). CONTENT would
        // write NONE for that field and the catalog rejects the row.
        db.query(
            "UPSERT type::record('plugin_workspaces', $name) SET
                name = $name,
                label = $label,
                group_label = $group_label,
                group_icon = $group_icon,
                description = $description,
                logo_file = $logo_file,
                launcher = $launcher;",
        )
        .bind(("name", workspace.name.clone()))
        .bind(("label", workspace.label.clone()))
        .bind(("group_label", workspace.group_label.clone()))
        .bind(("group_icon", icon_for_db))
        .bind(("description", workspace.description.clone()))
        .bind(("logo_file", logo_file))
        .bind(("launcher", launcher))
        .await
        .and_then(|response| response.check())
        .map_err(|error| WorkspaceError::Database(error.to_string()))?;
        log::info!(
            "registered workspace `{}` (group_label={:?})",
            workspace.name,
            workspace.group_label
        );
    }
    Ok(())
}

/// Absolute path to a staged workspace logo, if any.
///
/// Prefer the recorded `logo_file`; otherwise look for a conventional `logo.*`
/// under `workspaces/<name>/` (covers older rows that never stored `logo_file`).
pub async fn staged_logo_path(
    layout: &AppDir,
    name: &str,
    logo_file: Option<&str>,
) -> Option<PathBuf> {
    let dir = layout.workspace_dir(name).ok()?;
    if let Some(file) = logo_file.map(str::trim).filter(|v| !v.is_empty()) {
        let path = dir.join(file);
        if tokio::fs::try_exists(&path).await.unwrap_or(false) {
            return Some(path);
        }
    }
    for candidate in [
        "logo.svg",
        "logo.png",
        "logo.webp",
        "logo.jpg",
        "logo.jpeg",
        "logo.gif",
        "logo.ico",
    ] {
        let path = dir.join(candidate);
        if tokio::fs::try_exists(&path).await.unwrap_or(false) {
            return Some(path);
        }
    }
    None
}

/// Public URL for a workspace group logo when one is staged (or may be found by convention).
pub fn logo_url(name: &str, logo_file: Option<&str>) -> Option<String> {
    logo_file
        .filter(|v| !v.is_empty())
        .map(|_| format!("/api/ui/workspaces/{name}/logo"))
}

/// Advertise the workspace logo endpoint when a file logo is expected.
pub fn logo_url_for_workspace(
    name: &str,
    logo_file: Option<&str>,
    group_icon: Option<&str>,
) -> Option<String> {
    if logo_url(name, logo_file).is_some() {
        return Some(format!("/api/ui/workspaces/{name}/logo"));
    }
    let icon = group_icon.map(str::trim).filter(|v| !v.is_empty())?;
    if is_asset_icon(icon) {
        Some(format!("/api/ui/workspaces/{name}/logo"))
    } else {
        None
    }
}
