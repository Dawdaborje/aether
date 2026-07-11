use crate::errors::PluginError;
use crate::models::{Plugin, PluginWorkspace};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::fs;
use std::path::Path;

const PLUGIN_MANIFEST: &str = "plugin.toml";
const WORKSPACE_MANIFEST: &str = "workspace.toml";

/// On-disk representation of a `workspace.toml`.
///
/// This is intentionally decoupled from [`PluginWorkspace`] so the file format
/// can evolve independently from the in-memory model. Unknown keys are ignored.
#[derive(Debug, Deserialize)]
struct WorkspaceManifest {
    workspace: WorkspaceMeta,
    #[serde(default)]
    addons: Vec<AddonEntry>,
}

#[derive(Debug, Deserialize)]
struct WorkspaceMeta {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    group_label: String,
    version: Option<String>,
    author: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AddonEntry {
    name: String,
    path: String,
}

/// On-disk representation of a `plugin.toml`.
///
/// Everything is optional so an empty (or partial) manifest still parses, which
/// matches the current example plugins that only declare themselves through the
/// workspace's `[[addons]]` list.
#[derive(Debug, Default, Deserialize)]
struct PluginManifest {
    #[serde(default)]
    plugin: PluginMeta,
}

#[derive(Debug, Default, Deserialize)]
struct PluginMeta {
    name: Option<String>,
    description: Option<String>,
    long_description: Option<String>,
    version: Option<String>,
    author: Option<String>,
    license: Option<String>,
    homepage: Option<String>,
    repository: Option<String>,
    documentation: Option<String>,
    keywords: Option<Vec<String>>,
    categories: Option<Vec<String>>,
    tags: Option<Vec<String>>,
}

fn read_toml<T: DeserializeOwned>(path: &Path) -> Result<T, PluginError> {
    let content = fs::read_to_string(path).map_err(|source| PluginError::Read {
        path: path.display().to_string(),
        source,
    })?;

    toml::from_str(&content).map_err(|source| PluginError::Parse {
        path: path.display().to_string(),
        source,
    })
}

fn dir_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

/// Builds a [`Plugin`] from a directory.
///
/// A directory is treated as a plugin when it contains a `plugin.toml`, a
/// workspace when it contains a `workspace.toml`, and both when it contains
/// each. When a `workspace.toml` is present it is loaded recursively and
/// attached to [`Plugin::workspace`], which also handles nested workspaces.
///
/// `fallback_name` lets callers (e.g. a workspace listing its addons) supply a
/// name when the manifest does not declare one; otherwise the directory name is
/// used.
fn build_plugin_from_path(path: &Path, fallback_name: Option<&str>) -> Result<Plugin, PluginError> {
    let plugin_manifest = path.join(PLUGIN_MANIFEST);
    let workspace_manifest = path.join(WORKSPACE_MANIFEST);

    let has_plugin = plugin_manifest.is_file();
    let has_workspace = workspace_manifest.is_file();

    if !has_plugin && !has_workspace {
        return Err(PluginError::MissingManifest {
            path: path.display().to_string(),
        });
    }

    let meta = if has_plugin {
        read_toml::<PluginManifest>(&plugin_manifest)?.plugin
    } else {
        PluginMeta::default()
    };

    let name = meta
        .name
        .or_else(|| fallback_name.map(str::to_string))
        .unwrap_or_else(|| dir_name(path));

    let workspace = if has_workspace {
        Some(build_plugin_workspace_from_path(path)?)
    } else {
        None
    };

    Ok(Plugin {
        path: path.display().to_string(),
        name,
        workspace,
        description: meta.description,
        long_description: meta.long_description,
        version: meta.version,
        author: meta.author,
        license: meta.license,
        homepage: meta.homepage,
        repository: meta.repository,
        documentation: meta.documentation,
        keywords: meta.keywords,
        categories: meta.categories,
        tags: meta.tags,
    })
}

/// Reads the `workspace.toml` in `path` and produces a [`PluginWorkspace`],
/// resolving every declared addon into a [`Plugin`].
///
/// Addon paths are resolved relative to the workspace directory, and each addon
/// is loaded recursively so nested workspaces and plugins are supported.
fn build_plugin_workspace_from_path(path: &Path) -> Result<PluginWorkspace, PluginError> {
    let manifest_path = path.join(WORKSPACE_MANIFEST);
    let manifest: WorkspaceManifest = read_toml(&manifest_path)?;

    let mut plugins = Vec::with_capacity(manifest.addons.len());
    for addon in &manifest.addons {
        let addon_path = path.join(&addon.path);
        plugins.push(build_plugin_from_path(&addon_path, Some(&addon.name))?);
    }

    Ok(PluginWorkspace {
        path: path.display().to_string(),
        name: manifest.workspace.name,
        description: manifest.workspace.description,
        group_name: manifest.workspace.group_label,
        version: manifest.workspace.version,
        author: manifest.workspace.author,
        plugins,
    })
}

/// Loads a [`Plugin`] for each of the given paths.
///
/// A path may point at a plain plugin (`plugin.toml`), a workspace
/// (`workspace.toml`), or a directory that is both, in which case the resolved
/// [`Plugin`] carries its [`Plugin::workspace`]. Missing or malformed manifests
/// produce a [`PluginError`].
pub fn get_plugins_from_paths(paths: Vec<String>) -> Result<Vec<Plugin>, PluginError> {
    let mut plugins = Vec::with_capacity(paths.len());
    for path in paths {
        plugins.push(build_plugin_from_path(Path::new(&path), None)?);
    }

    Ok(plugins)
}
