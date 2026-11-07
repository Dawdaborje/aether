//! Turn `--load-plugins` / `plugin_paths` entries into package directories.
//!
//! Each entry may be a plugin package, a workspace directory (`workspace.toml`),
//! or a workspace name such as `base` (looked up under the search roots and
//! `plugins/<name>`). Workspace members are expanded, then the whole set is
//! ordered so a plugin's `dependencies` load before it.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::models::plugin_def::PluginManifest;
use super::models::plugin_workspace_def::PluginWorkspaceManifest;

const PLUGIN_MANIFEST: &str = "plugin.toml";
const WORKSPACE_MANIFEST: &str = "workspace.toml";

#[derive(Debug, thiserror::Error)]
pub enum LoadPathsError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid workspace manifest {path}: {source}")]
    WorkspaceManifest {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("invalid plugin manifest {path}: {source}")]
    PluginManifest {
        path: PathBuf,
        #[source]
        source: super::models::plugin_def::ManifestError,
    },
    #[error(
        "nothing found for `{0}`: expected a plugin package, a workspace directory, or a workspace name under ./ or ./plugins/"
    )]
    NotFound(String),
    #[error("workspace `{name}` at {path} has no plugins under [workspace.plugins]")]
    EmptyWorkspace { name: String, path: PathBuf },
    #[error("workspace member `{name}` at {path} is missing {PLUGIN_MANIFEST}")]
    MissingMember { name: String, path: PathBuf },
    #[error("dependency cycle involving `{0}`")]
    DependencyCycle(String),
    #[error("duplicate plugin name `{name}` while loading (`{first}` and `{second}`)")]
    DuplicatePlugin {
        name: String,
        first: PathBuf,
        second: PathBuf,
    },
}

/// One workspace.toml discovered while resolving load paths.
#[derive(Debug, Clone)]
pub struct WorkspaceSource {
    pub dir: PathBuf,
    pub name: String,
    pub label: Option<String>,
    pub group_label: Option<String>,
    pub group_icon: Option<String>,
    pub description: Option<String>,
    /// Apps home layout: `folder` (default) or `flat`.
    pub launcher: Option<String>,
}

/// Packages to load plus the workspaces they came from (for launcher groups).
#[derive(Debug, Clone)]
pub struct LoadPlan {
    pub packages: Vec<PathBuf>,
    pub workspaces: Vec<WorkspaceSource>,
}

#[derive(Debug, Default)]
struct PathHit {
    packages: Vec<PathBuf>,
    workspace: Option<WorkspaceSource>,
}

/// Expand `entries` to concrete plugin package directories, dependencies first.
///
/// `search_roots` are used when an entry is a bare workspace name (for example
/// `base`): each root is tried as `<root>/<name>` and `<root>/plugins/<name>`.
/// Workspaces encountered along the way are returned for catalog registration.
pub async fn resolve_load_paths(
    entries: &[PathBuf],
    search_roots: &[PathBuf],
) -> Result<LoadPlan, LoadPathsError> {
    let mut packages = Vec::new();
    let mut seen_dirs = HashSet::new();
    let mut workspaces = Vec::new();
    let mut seen_workspaces = HashSet::new();

    for entry in entries {
        let hit = resolve_entry(entry, search_roots).await?;
        if let Some(workspace) = hit.workspace {
            if seen_workspaces.insert(workspace.name.clone()) {
                workspaces.push(workspace);
            }
        }
        for package in hit.packages {
            let key = canonicalize_key(&package);
            if seen_dirs.insert(key) {
                packages.push(package);
            }
        }
    }

    Ok(LoadPlan {
        packages: order_by_dependencies(&packages).await?,
        workspaces,
    })
}

async fn resolve_entry(entry: &Path, search_roots: &[PathBuf]) -> Result<PathHit, LoadPathsError> {
    if let Some(hit) = try_path(entry).await? {
        return Ok(hit);
    }

    let name = entry
        .file_name()
        .and_then(|part| part.to_str())
        .filter(|name| !name.is_empty() && *name != "." && *name != "..")
        .ok_or_else(|| LoadPathsError::NotFound(entry.display().to_string()))?;

    // Bare names, or a relative path that does not exist yet: look under the
    // search roots (and `plugins/<name>`), matching `aether --load-plugins base`.
    if entry.components().count() <= 1 || !path_exists(entry).await {
        for root in search_roots {
            for candidate in [root.join(name), root.join("plugins").join(name)] {
                if let Some(hit) = try_path(&candidate).await? {
                    return Ok(hit);
                }
            }
        }
    }

    Err(LoadPathsError::NotFound(entry.display().to_string()))
}

async fn try_path(path: &Path) -> Result<Option<PathHit>, LoadPathsError> {
    if !path_exists(path).await {
        return Ok(None);
    }

    let meta = tokio::fs::metadata(path)
        .await
        .map_err(|source| LoadPathsError::Io {
            path: path.to_path_buf(),
            source,
        })?;

    if meta.is_file() {
        let file_name = path.file_name().and_then(|name| name.to_str());
        return match file_name {
            Some(PLUGIN_MANIFEST) => Ok(Some(PathHit {
                packages: vec![path.parent().unwrap_or(Path::new(".")).to_path_buf()],
                workspace: None,
            })),
            Some(WORKSPACE_MANIFEST) => {
                let dir = path.parent().unwrap_or(Path::new("."));
                Ok(Some(expand_workspace(dir).await?))
            }
            _ => Ok(None),
        };
    }

    if path.join(WORKSPACE_MANIFEST).is_file() {
        return Ok(Some(expand_workspace(path).await?));
    }
    if path.join(PLUGIN_MANIFEST).is_file() {
        return Ok(Some(PathHit {
            packages: vec![path.to_path_buf()],
            workspace: None,
        }));
    }
    Ok(None)
}

async fn expand_workspace(dir: &Path) -> Result<PathHit, LoadPathsError> {
    let manifest_path = dir.join(WORKSPACE_MANIFEST);
    let text = tokio::fs::read_to_string(&manifest_path)
        .await
        .map_err(|source| LoadPathsError::Io {
            path: manifest_path.clone(),
            source,
        })?;
    let manifest: PluginWorkspaceManifest =
        toml::from_str(&text).map_err(|source| LoadPathsError::WorkspaceManifest {
            path: manifest_path.clone(),
            source,
        })?;

    if manifest.workspace.plugins.is_empty() {
        return Err(LoadPathsError::EmptyWorkspace {
            name: manifest.workspace.name,
            path: dir.to_path_buf(),
        });
    }

    let mut packages = Vec::with_capacity(manifest.workspace.plugins.len());
    for (name, member) in &manifest.workspace.plugins {
        let package = dir.join(&member.path);
        if !package.join(PLUGIN_MANIFEST).is_file() {
            return Err(LoadPathsError::MissingMember {
                name: name.clone(),
                path: package,
            });
        }
        packages.push(package);
    }
    Ok(PathHit {
        packages,
        workspace: Some(WorkspaceSource {
            dir: dir.to_path_buf(),
            name: manifest.workspace.name,
            label: manifest.workspace.label,
            group_label: manifest.workspace.group_label,
            group_icon: manifest.workspace.group_icon,
            description: manifest.workspace.description,
            launcher: manifest.workspace.launcher,
        }),
    })
}

struct Package {
    path: PathBuf,
    name: String,
    dependencies: Vec<String>,
}

async fn order_by_dependencies(packages: &[PathBuf]) -> Result<Vec<PathBuf>, LoadPathsError> {
    let mut loaded = Vec::with_capacity(packages.len());
    for path in packages {
        loaded.push(read_package(path).await?);
    }

    let mut by_name: HashMap<String, usize> = HashMap::new();
    for (index, package) in loaded.iter().enumerate() {
        if let Some(previous) = by_name.insert(package.name.clone(), index) {
            return Err(LoadPathsError::DuplicatePlugin {
                name: package.name.clone(),
                first: loaded[previous].path.clone(),
                second: package.path.clone(),
            });
        }
    }

    let mut order = Vec::with_capacity(loaded.len());
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();

    fn visit(
        index: usize,
        loaded: &[Package],
        by_name: &HashMap<String, usize>,
        visiting: &mut HashSet<String>,
        visited: &mut HashSet<String>,
        order: &mut Vec<PathBuf>,
    ) -> Result<(), LoadPathsError> {
        let package = &loaded[index];
        if visited.contains(&package.name) {
            return Ok(());
        }
        if !visiting.insert(package.name.clone()) {
            return Err(LoadPathsError::DependencyCycle(package.name.clone()));
        }
        for dependency in &package.dependencies {
            if let Some(&dep_index) = by_name.get(dependency) {
                visit(dep_index, loaded, by_name, visiting, visited, order)?;
            }
        }
        visiting.remove(&package.name);
        visited.insert(package.name.clone());
        order.push(package.path.clone());
        Ok(())
    }

    for index in 0..loaded.len() {
        visit(
            index,
            &loaded,
            &by_name,
            &mut visiting,
            &mut visited,
            &mut order,
        )?;
    }
    Ok(order)
}

async fn read_package(path: &Path) -> Result<Package, LoadPathsError> {
    let manifest_path = if path.is_file() {
        path.to_path_buf()
    } else {
        path.join(PLUGIN_MANIFEST)
    };
    let package_dir = if path.is_file() {
        path.parent().unwrap_or(Path::new(".")).to_path_buf()
    } else {
        path.to_path_buf()
    };
    let text = tokio::fs::read_to_string(&manifest_path)
        .await
        .map_err(|source| LoadPathsError::Io {
            path: manifest_path.clone(),
            source,
        })?;
    let manifest =
        PluginManifest::parse(&text).map_err(|source| LoadPathsError::PluginManifest {
            path: manifest_path,
            source,
        })?;
    Ok(Package {
        path: package_dir,
        name: manifest.plugin.name,
        dependencies: manifest.plugin.dependencies,
    })
}

async fn path_exists(path: &Path) -> bool {
    tokio::fs::metadata(path).await.is_ok()
}

fn canonicalize_key(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, text).unwrap();
    }

    fn plugin_toml(name: &str, deps: &[&str]) -> String {
        let deps = deps
            .iter()
            .map(|dep| format!("\"{dep}\""))
            .collect::<Vec<_>>()
            .join(", ");
        format!("[plugin]\nname = \"{name}\"\nversion = \"0.1.0\"\ndependencies = [{deps}]\n")
    }

    #[tokio::test]
    async fn expands_a_workspace_and_orders_by_dependencies() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("plugins").join("base");
        write(
            &base.join("workspace.toml"),
            r#"
[workspace]
name = "base"

[workspace.plugins]
company = { path = "./company" }
currency = { path = "./currency" }
country = { path = "./country" }
"#,
        );
        write(
            &base.join("currency/plugin.toml"),
            &plugin_toml("currency", &[]),
        );
        write(
            &base.join("country/plugin.toml"),
            &plugin_toml("country", &["currency"]),
        );
        write(
            &base.join("company/plugin.toml"),
            &plugin_toml("company", &["country"]),
        );

        let plan = resolve_load_paths(&[PathBuf::from("base")], &[dir.path().to_path_buf()])
            .await
            .unwrap();

        let names: Vec<_> = plan
            .packages
            .iter()
            .map(|path| path.file_name().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(names, ["currency", "country", "company"]);
        assert_eq!(plan.workspaces.len(), 1);
        assert_eq!(plan.workspaces[0].name, "base");
    }

    #[tokio::test]
    async fn keeps_a_direct_package_path() {
        let dir = tempfile::tempdir().unwrap();
        let package = dir.path().join("notes");
        write(&package.join("plugin.toml"), &plugin_toml("notes", &[]));

        let plan = resolve_load_paths(&[package.clone()], &[dir.path().to_path_buf()])
            .await
            .unwrap();
        assert_eq!(plan.packages, [package]);
        assert!(plan.workspaces.is_empty());
    }

    #[tokio::test]
    async fn expands_a_workspace_directory_path() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("base");
        write(
            &base.join("workspace.toml"),
            r#"
[workspace]
name = "base"
group_label = "Base"
group_icon = "./assets/base.svg"
[workspace.plugins]
currency = { path = "./currency" }
"#,
        );
        write(
            &base.join("currency/plugin.toml"),
            &plugin_toml("currency", &[]),
        );

        let plan = resolve_load_paths(&[base.clone()], &[]).await.unwrap();
        assert_eq!(plan.packages, [base.join("currency")]);
        assert_eq!(plan.workspaces[0].group_label.as_deref(), Some("Base"));
        assert_eq!(
            plan.workspaces[0].group_icon.as_deref(),
            Some("./assets/base.svg")
        );
    }

    #[tokio::test]
    async fn rejects_an_unknown_name() {
        let dir = tempfile::tempdir().unwrap();
        let err = resolve_load_paths(&[PathBuf::from("missing")], &[dir.path().to_path_buf()])
            .await
            .unwrap_err();
        assert!(matches!(err, LoadPathsError::NotFound(_)), "{err}");
    }
}
