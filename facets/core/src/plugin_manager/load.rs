//! Load plugins into the core catalog (CLI `--load-plugins` and Desk Apps).

use std::io;
use std::path::{Component, Path, PathBuf};

use surrealdb::{Surreal, engine::remote::ws::Client};
use thiserror::Error;
use zip::ZipArchive;

use crate::cache::Cache;
use crate::plugin_manager::catalog::{self, CatalogError, LoadedPlugin};
use crate::plugin_manager::load_paths::{self, LoadPathsError};
use crate::plugin_manager::mantle_signals::{self, SignalKind};
use crate::plugin_manager::surface;
use crate::plugin_manager::workspaces::{self, WorkspaceError};

const PLUGIN_MANIFEST: &str = "plugin.toml";
const WORKSPACE_MANIFEST: &str = "workspace.toml";

/// Reject archives whose compressed payload is larger than this.
pub const MAX_ZIP_BYTES: usize = 50 * 1024 * 1024;
/// Cap total uncompressed size to limit zip bombs.
const MAX_UNCOMPRESSED_BYTES: u64 = 200 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum CatalogLoadError {
    #[error("{0}")]
    Paths(#[from] LoadPathsError),
    #[error("{0}")]
    Workspace(#[from] WorkspaceError),
    #[error("{0}")]
    Catalog(#[from] CatalogError),
}

#[derive(Debug, Error)]
pub enum UnpackError {
    #[error("{0}")]
    Message(String),
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Resolve `entries`, register workspaces, load packages into the catalog.
///
/// When `cache` is set, Mantle surface entries are invalidated in-process.
/// A DB signal is always emitted for new revisions so a separate `--serve`
/// process can bust its own Mantle (CLI and server do not share memory).
/// Does **not** upgrade organizations (unlike `--serve --watch`).
pub async fn load_into_catalog(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    app_dir: &Path,
    cache: Option<&Cache>,
    entries: &[PathBuf],
    search_roots: &[PathBuf],
) -> Result<Vec<LoadedPlugin>, CatalogLoadError> {
    let plan = load_paths::resolve_load_paths(entries, search_roots).await?;
    workspaces::register_workspaces(db, namespace, core_database, app_dir, &plan.workspaces)
        .await?;
    let batch_models = catalog::batch_model_ids(&plan.packages)?;
    let mut loaded = Vec::with_capacity(plan.packages.len());
    for plugin_path in &plan.packages {
        let plugin = catalog::load_plugin_with_batch(
            db,
            namespace,
            core_database,
            app_dir,
            plugin_path,
            &batch_models,
        )
        .await?;
        if plugin.created {
            if let Some(cache) = cache {
                surface::invalidate(cache, &plugin.name, &plugin.version);
            }
            mantle_signals::emit(
                db,
                namespace,
                core_database,
                SignalKind::PluginSurface,
                Some(&plugin.name),
                Some(&plugin.version),
                None,
            )
            .await;
        }
        loaded.push(plugin);
    }
    Ok(loaded)
}

/// Whether a zip entry path is safe to extract under `dest` (no absolute / `..`).
pub fn safe_zip_relative(name: &str) -> Result<PathBuf, UnpackError> {
    let path = Path::new(name);
    if path.is_absolute() {
        return Err(UnpackError::Message(format!(
            "zip entry `{name}` must not be an absolute path"
        )));
    }
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(UnpackError::Message(format!(
                    "zip entry `{name}` must not contain `..`"
                )));
            }
            Component::Prefix(_) | Component::RootDir => {
                return Err(UnpackError::Message(format!(
                    "zip entry `{name}` must not be an absolute path"
                )));
            }
        }
    }
    if clean.as_os_str().is_empty() {
        return Err(UnpackError::Message(format!(
            "zip entry `{name}` has an empty path"
        )));
    }
    Ok(clean)
}

/// Unpack `bytes` into `dest` and return the package or workspace root.
///
/// Accepts an archive whose root is `plugin.toml` / `workspace.toml`, or a single
/// top-level directory that contains one of those manifests.
pub fn unpack_plugin_zip(bytes: &[u8], dest: &Path) -> Result<PathBuf, UnpackError> {
    if bytes.len() > MAX_ZIP_BYTES {
        return Err(UnpackError::Message(format!(
            "zip is larger than {} MB",
            MAX_ZIP_BYTES / (1024 * 1024)
        )));
    }
    std::fs::create_dir_all(dest).map_err(|source| UnpackError::Io {
        path: dest.to_path_buf(),
        source,
    })?;

    let cursor = io::Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor)?;
    let mut uncompressed: u64 = 0;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let Some(enclosed) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            return Err(UnpackError::Message(format!(
                "zip entry `{}` is not a safe path",
                entry.name()
            )));
        };
        let relative = safe_zip_relative(&enclosed.to_string_lossy())?;
        let out = dest.join(&relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|source| UnpackError::Io {
                path: out.clone(),
                source,
            })?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|source| UnpackError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let size = entry.size();
        uncompressed = uncompressed.saturating_add(size);
        if uncompressed > MAX_UNCOMPRESSED_BYTES {
            return Err(UnpackError::Message(format!(
                "zip unpack would exceed {} MB uncompressed",
                MAX_UNCOMPRESSED_BYTES / (1024 * 1024)
            )));
        }
        let mut file = std::fs::File::create(&out).map_err(|source| UnpackError::Io {
            path: out.clone(),
            source,
        })?;
        io::copy(&mut entry, &mut file).map_err(|source| UnpackError::Io {
            path: out.clone(),
            source,
        })?;
    }

    find_package_root(dest)
}

fn find_package_root(dest: &Path) -> Result<PathBuf, UnpackError> {
    if dest.join(PLUGIN_MANIFEST).is_file() || dest.join(WORKSPACE_MANIFEST).is_file() {
        return Ok(dest.to_path_buf());
    }
    let mut dirs = Vec::new();
    let entries = std::fs::read_dir(dest).map_err(|source| UnpackError::Io {
        path: dest.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| UnpackError::Io {
            path: dest.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.is_dir() {
            dirs.push(path);
        }
    }
    if dirs.len() == 1 {
        let only = &dirs[0];
        if only.join(PLUGIN_MANIFEST).is_file() || only.join(WORKSPACE_MANIFEST).is_file() {
            return Ok(only.clone());
        }
    }
    Err(UnpackError::Message(
        "zip must contain plugin.toml or workspace.toml at the root (or in a single top-level folder)"
            .into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    #[test]
    fn rejects_parent_dir_in_zip_name() {
        assert!(safe_zip_relative("../evil.toml").is_err());
        assert!(safe_zip_relative("foo/../../evil.toml").is_err());
    }

    #[test]
    fn rejects_absolute_zip_name() {
        assert!(safe_zip_relative("/tmp/evil.toml").is_err());
    }

    #[test]
    fn accepts_normal_relative_name() {
        assert_eq!(
            safe_zip_relative("pkg/plugin.toml").unwrap(),
            PathBuf::from("pkg/plugin.toml")
        );
    }

    #[test]
    fn unpacks_nested_plugin_root() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("plugin.zip");
        {
            let file = std::fs::File::create(&zip_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options = SimpleFileOptions::default();
            zip.add_directory("demo/", options).unwrap();
            zip.start_file("demo/plugin.toml", options).unwrap();
            zip.write_all(
                br#"
name = "demo"
version = "0.1.0"
label = "Demo"
"#,
            )
            .unwrap();
            zip.finish().unwrap();
        }
        let bytes = std::fs::read(&zip_path).unwrap();
        let dest = dir.path().join("out");
        let root = unpack_plugin_zip(&bytes, &dest).unwrap();
        assert!(root.join("plugin.toml").is_file());
        assert_eq!(root.file_name().unwrap(), "demo");
    }
}
