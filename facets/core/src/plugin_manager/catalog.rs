//! Plugin catalog operations used by the CLI.
//!
//! * [`load_plugin`] registers a plugin package (a directory holding
//!   `plugin.toml` and its WASM artifact) in the **core** `plugins` catalog.
//! * [`install_plugins`] records catalog plugins, and the dependencies they
//!   need, in one organization's `installed_plugins` table.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use surrealdb::{Surreal, engine::remote::ws::Client, types::SurrealValue};
use aether_security::capabilities::CapabilityCatalog;
use thiserror::Error;

use crate::app_dir::{AppDir, AppDirError};
use super::models::plugin_db_def::{PluginDbAuthor, PluginDbCategory, PluginDbDefinition};
use super::models::plugin_def::{ManifestError, PluginManifest};
use super::pages::{PageDocument, PageError, discover_pages, write_view_files};
use super::revisions;
use crate::data_model::{ModelDef, ModelFileError, apply as model_apply, read_models};
use super::themes::{ThemeDocument, ThemeError, parse_theme};

const MANIFEST_FILE: &str = "plugin.toml";
const INSTALLED_BY_CLI: &str = "cli";

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("database error: {0}")]
    Database(#[from] surrealdb::Error),

    #[error("{0}")]
    ForeignLink(String),

    #[error("the schedules of `{0}` could not be set up: {1}")]
    Schedule(String, String),

    #[error("filesystem error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("invalid manifest {path}: {source}")]
    Manifest {
        path: PathBuf,
        #[source]
        source: ManifestError,
    },

    #[error(transparent)]
    Page(#[from] PageError),

    #[error("i18n: {0}")]
    I18n(String),

    #[error(transparent)]
    Theme(#[from] ThemeError),

    #[error("{path}: `[app].route` is `{route}`, which is not a page of this plugin (pages: {pages})")]
    AppRouteNotAPage {
        path: PathBuf,
        route: String,
        pages: String,
    },

    #[error("{path}: `[app].{field}` is invalid: {reason}")]
    InvalidApp {
        path: PathBuf,
        field: &'static str,
        reason: &'static str,
    },

    #[error("{0}: `[theme]` needs `tokens_file`")]
    ThemeWithoutTokens(PathBuf),

    #[error("theme `{0}` is not installed in this organization")]
    ThemeNotInstalled(String),

    #[error("{page}: page uses model `{model}`, which the plugin does not declare")]
    UnknownPageModel { page: PathBuf, model: String },

    #[error(
        "route `{route}` is already served by `{existing}`, so `{plugin}` cannot be installed in the same organization"
    )]
    RouteConflict {
        route: String,
        existing: String,
        plugin: String,
    },

    #[error("manifest {0} must declare a non-empty plugin name and version")]
    IncompleteManifest(PathBuf),

    #[error("manifest {0} does not declare `plugin.wasm_file` or `plugin.script`")]
    MissingWasmFile(PathBuf),

    #[error("manifest {0} declares both `plugin.wasm_file` and `plugin.script`; a plugin has one")]
    BothWasmAndScript(PathBuf),

    #[error("script {path} does not compile: {message}")]
    ScriptInvalid { path: PathBuf, message: String },

    #[error("script {path} must end in `.rhai`")]
    ScriptExtension { path: PathBuf },

    #[error("{path} is not a regular file inside the plugin package {package}")]
    FileOutsidePackage { path: PathBuf, package: PathBuf },

    #[error(transparent)]
    AppDir(#[from] AppDirError),

    #[error("manifest path {0} has no parent directory")]
    InvalidManifestPath(PathBuf),

    #[error("plugin `{0}` is not installed in this organization; install it first")]
    NotInstalled(String),

    #[error("plugin `{plugin}` needs `{dependency}`, which this organization has not installed")]
    MissingDependency { plugin: String, dependency: String },

    #[error("invalid plugin spec `{0}`; expected `name` or `name@version`")]
    InvalidSpec(String),

    #[error("{path}: {source}; capabilities are listed in `capabilities/`")]
    UnknownCapability {
        path: PathBuf,
        #[source]
        source: aether_security::capabilities::CapabilityError,
    },

    #[error("{path}: `public_capabilities` lists `{capability}`, which the plugin does not list in `capabilities`")]
    PublicCapabilityNotHeld { path: PathBuf, capability: String },

    #[error("the built-in capability catalog is invalid: {0}")]
    CapabilityCatalog(String),

    #[error(transparent)]
    Revision(#[from] super::revisions::RevisionError),

    #[error(transparent)]
    ModelFile(#[from] ModelFileError),

    #[error(transparent)]
    ModelSchema(#[from] model_apply::ApplyError),

    #[error("{path}: `{name}` is listed in `{list}` but there is no model of that name; define it in `models/{name}.json`")]
    UnknownGrantedModel { path: PathBuf, name: String, list: &'static str },

    #[error("model id `{model_id}` ({model}) is already used by plugin `{plugin}`; ids must be unique across plugins")]
    ModelIdTaken { model_id: String, model: String, plugin: String },

    #[error("organization database `{0}` was not found")]
    OrganizationNotFound(String),

    #[error("plugin `{0}` is not in the catalog")]
    PluginNotFound(String),

    #[error("plugin `{name}@{version}` is not an active catalog version")]
    VersionNotFound { name: String, version: String },

    #[error("plugin `{name}` is already installed at {installed}, but {requested} was requested")]
    VersionMismatch {
        name: String,
        installed: String,
        requested: String,
    },

    #[error("dependency cycle involving `{0}`")]
    DependencyCycle(String),
}

fn io_error(path: &Path) -> impl FnOnce(io::Error) -> CatalogError + '_ {
    move |source| CatalogError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// A plugin named on the command line: `name` or `name@version`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginSpec {
    pub name: String,
    pub version: Option<String>,
}

impl FromStr for PluginSpec {
    type Err = CatalogError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let (name, version) = match raw.split_once('@') {
            Some((name, version)) => (name, Some(version)),
            None => (raw, None),
        };
        let name = name.trim();
        let version = version.map(str::trim);
        if name.is_empty() || version.is_some_and(str::is_empty) {
            return Err(CatalogError::InvalidSpec(raw.to_string()));
        }
        Ok(Self {
            name: name.to_string(),
            version: version.map(str::to_string),
        })
    }
}

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Debug, Clone, Serialize, SurrealValue)]
struct NewPlugin {
    name: String,
    label: String,
    version: String,
    description: Option<String>,
    long_description: Option<String>,
    icon_path: Option<String>,
    website: Option<String>,
    authors: Vec<PluginDbAuthor>,
    categories: Vec<PluginDbCategory>,
    dependencies: Vec<String>,
    workspace: Option<String>,
    kind: Option<String>,
    artifact_path: Option<String>,
    artifact_hash: Option<String>,
    content_hash: String,
    /// The folder under `plugins/<name>/` holding this version's file index.
    revision: Option<String>,
    /// Where the package was loaded from.
    source_path: Option<String>,
    /// The launcher tile, when the plugin is an app.
    app: Option<NewApp>,
    /// Functions anonymous visitors may call; checked before a module is compiled.
    public_functions: Vec<String>,
    /// The manifest's `[[schedule]]` entries.
    schedules: Vec<serde_json::Value>,
    /// The manifest's `[[command]]` entries.
    commands: Vec<serde_json::Value>,
    /// The plugin's translated text, from `i18n/<locale>.json`.
    i18n: Option<serde_json::Value>,
    /// The manifest's `[[watch]]` entries.
    watches: Vec<serde_json::Value>,
    /// The events the manifest listens to.
    event_listeners: Vec<serde_json::Value>,
    is_builtin: bool,
    is_active: bool,
}

#[derive(Debug, Clone, Serialize, SurrealValue)]
struct NewApp {
    label: String,
    icon: Option<String>,
    route: String,
    description: Option<String>,
}

#[derive(Debug, Clone, Serialize, SurrealValue)]
struct NewModelRow {
    name: String,
    model_id: String,
    label: Option<String>,
    definition: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, SurrealValue)]
struct NewTheme {
    name: String,
    label: String,
    is_system: bool,
    color_mode: String,
    tokens: serde_json::Value,
    layout: String,
    error_pages: String,
    nav: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, SurrealValue)]
struct NewUiPage {
    layout: Option<String>,
    route: String,
    is_pattern: bool,
    route_shape: String,
    title: String,
    model: Option<String>,
    models: Vec<String>,
    is_public: bool,
    component_tree: serde_json::Value,
    source_path: String,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct KnownVersion {
    version: String,
    content_hash: Option<String>,
    revision: Option<String>,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct ModelIdOwner {
    model_id: String,
    plugin: String,
}

/// `0.1.0+20261003T210100Z` is a rebuild of `0.1.0`.
pub fn base_version(version: &str) -> &str {
    version.split('+').next().unwrap_or(version)
}

/// A package-relative path as it is written in `files.json`.
fn logical_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// The capabilities a manifest asks for must exist, and a public capability must be one the
/// plugin holds.
fn validate_capabilities(manifest: &PluginManifest, manifest_path: &Path) -> Result<(), CatalogError> {
    use std::sync::OnceLock;
    static CATALOG: OnceLock<Result<CapabilityCatalog, String>> = OnceLock::new();
    let catalog = CATALOG
        .get_or_init(|| CapabilityCatalog::builtin().map_err(|error| error.to_string()))
        .as_ref()
        .map_err(|message| CatalogError::CapabilityCatalog(message.clone()))?;
    let plugin = &manifest.plugin;
    catalog
        .validate_declared(&plugin.capabilities, &plugin.public_capabilities)
        .map_err(|source| CatalogError::UnknownCapability {
            path: manifest_path.to_path_buf(),
            source,
        })?;
    if let Some(capability) = plugin
        .public_capabilities
        .iter()
        .find(|capability| !plugin.capabilities.contains(capability))
    {
        return Err(CatalogError::PublicCapabilityNotHeld {
            path: manifest_path.to_path_buf(),
            capability: capability.clone(),
        });
    }
    Ok(())
}

#[derive(Debug, Deserialize, SurrealValue)]
struct InstalledRow {
    plugin_name: String,
    version: String,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct OrgDatabaseRow {
    #[allow(dead_code)]
    db_name: String,
}

/// Result of [`load_plugin`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedPlugin {
    pub name: String,
    pub version: String,
    /// Relative to `app_dir`; `None` for a plugin without a WASM artifact.
    pub artifact_path: Option<String>,
    pub pages: usize,
    pub public_pages: usize,
    /// `(name, layout)` when the plugin ships a theme.
    pub theme: Option<(String, String)>,
    /// `(label, route)` when the plugin is an app.
    pub app: Option<(String, String)>,
    /// `false` when identical content was already in the catalog.
    pub created: bool,
    /// The time-stamped folder this load wrote (or, for content already catalogued, the one
    /// it is in); `None` for a plugin stored before revisions existed.
    pub revision: Option<String>,
    /// Files written for this load: new or changed since the previous revision.
    pub written_files: usize,
    /// Files that did not change and were not copied again.
    pub reused_files: usize,
}

/// Register the plugin package at `plugin_path` in the core catalog.
///
/// `plugin_path` is a package directory (or its `plugin.toml`) anywhere on
/// disk. Pages are the XML files under `pages/`; each is parsed and validated
/// here, so a malformed page or a duplicate route fails the load. The files a
/// plugin ships (`plugin.toml`, page XML, model and theme files, and the WASM
/// artifact, which may sit in a build folder such as `out/` and is stored
/// beside `plugin.toml`) are stored under `<app_dir>/plugins/<name>/`, and each page is
/// compiled to JSON under `<app_dir>/views/`. The catalog records the plugin and its pages
/// together.
///
/// Files are stored as revisions (see [`super::revisions`]): they are compared by hash with
/// the plugin's latest revision, and only the ones that differ are written, into a new
/// folder named after the date and time. Loading identical content again is a no-op; loading
/// changed content under a version that is already catalogued adds `<version>+<stamp>`, so a
/// rebuild needs no version bump. A plugin may have no WASM artifact (a pure UI plugin).
pub async fn load_plugin(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    app_dir: &Path,
    plugin_path: &Path,
) -> Result<LoadedPlugin, CatalogError> {
    let layout = AppDir::new(app_dir);
    layout.ensure().await?;

    let manifest_path = if plugin_path.is_dir() {
        plugin_path.join(MANIFEST_FILE)
    } else {
        plugin_path.to_path_buf()
    };
    let manifest_path = tokio::fs::canonicalize(&manifest_path)
        .await
        .map_err(io_error(&manifest_path))?;
    let package_dir = manifest_path
        .parent()
        .ok_or_else(|| CatalogError::InvalidManifestPath(manifest_path.clone()))?
        .to_path_buf();

    let manifest_text = tokio::fs::read_to_string(&manifest_path)
        .await
        .map_err(io_error(&manifest_path))?;
    let manifest =
        PluginManifest::parse(&manifest_text).map_err(|source| CatalogError::Manifest {
            path: manifest_path.clone(),
            source,
        })?;
    if manifest.plugin.name.trim().is_empty() || manifest.plugin.version.trim().is_empty() {
        return Err(CatalogError::IncompleteManifest(manifest_path));
    }

    let pages = discover_pages(&package_dir).await?;
    let model_files = read_models(&package_dir)?;
    let models: Vec<ModelDef> = model_files.iter().map(|(_, model)| model.clone()).collect();
    check_foreign_links(db, namespace, core_database, &manifest.plugin, &models).await?;
    validate_page_models(&manifest, &models, &pages)?;
    validate_granted_models(&manifest, &models, &manifest_path)?;
    validate_capabilities(&manifest, &manifest_path)?;

    let theme = read_theme(&manifest, &manifest_path, &package_dir).await?;
    let app = read_app(&manifest, &manifest_path, &pages)?;

    let mut files = BTreeSet::from([PathBuf::from(MANIFEST_FILE)]);
    files.extend(pages.iter().map(|page| page.source.clone()));
    for (path, _) in &model_files {
        if let Ok(relative) = path.strip_prefix(&package_dir) {
            files.insert(relative.to_path_buf());
        }
    }
    for declared in declared_files(&manifest) {
        files.insert(resolve_package_file(&package_dir, declared).await?);
    }
    let text_catalogs = super::i18n::read_catalogs(&package_dir, &manifest)
        .await
        .map_err(CatalogError::I18n)?;
    if let Some(found) = &text_catalogs {
        super::i18n::check_overrides(&manifest, &found.catalogs).map_err(CatalogError::I18n)?;
        files.extend(found.files.iter().cloned());
    }

    // The artifact may live in a build folder such as `out/`; it is stored beside
    // `plugin.toml`, under its own file name.
    let declares = |file: &Option<String>| file.as_deref().is_some_and(|file| !file.is_empty());
    if declares(&manifest.plugin.wasm_file) && declares(&manifest.plugin.script) {
        return Err(CatalogError::BothWasmAndScript(manifest_path.clone()));
    }
    let artifact = match manifest.plugin.code_file() {
        Some(code_file) => {
            let source = resolve_package_file(&package_dir, code_file).await?;
            if manifest.plugin.is_script() && source.extension().is_none_or(|extension| extension != "rhai") {
                return Err(CatalogError::ScriptExtension { path: source });
            }
            let name = source
                .file_name()
                .map(PathBuf::from)
                .ok_or_else(|| CatalogError::MissingWasmFile(manifest_path.clone()))?;
            Some(Artifact { source, name })
        }
        None => None,
    };

    // Read every file once: these bytes are what is hashed, compared and stored.
    let mut contents: Vec<(String, Vec<u8>)> = Vec::new();
    let mut wasm_hash = None;
    let mut content = Sha256::new();
    for relative in &files {
        let source = package_dir.join(relative);
        let bytes = tokio::fs::read(&source).await.map_err(io_error(&source))?;
        content.update(relative.to_string_lossy().as_bytes());
        content.update((bytes.len() as u64).to_le_bytes());
        content.update(&bytes);
        contents.push((logical_path(relative), bytes));
    }
    let artifact_logical = artifact.as_ref().map(|artifact| logical_path(&artifact.name));
    if let Some(artifact) = &artifact {
        let source = package_dir.join(&artifact.source);
        let bytes = tokio::fs::read(&source).await.map_err(io_error(&source))?;
        // A script that does not compile is refused now, not at the first call.
        if manifest.plugin.is_script() {
            let text = String::from_utf8_lossy(&bytes);
            super::script::ScriptProgram::compile(&text)
                .map_err(|error| CatalogError::ScriptInvalid { path: source.clone(), message: error.to_string() })?;
        }
        wasm_hash = Some(sha256_hex(&bytes));
        content.update(artifact.name.to_string_lossy().as_bytes());
        content.update(&bytes);
        contents.push((logical_path(&artifact.name), bytes));
    }
    let content_hash = content
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    let is_system_theme = manifest.theme.as_ref().is_some_and(|theme| theme.is_system);
    let app_summary = app.as_ref().map(|app| (app.label.clone(), app.route.clone()));
    let theme_summary = theme
        .as_ref()
        .map(|theme| (theme.name.clone(), theme.layout.clone()));
    let schedules: Vec<serde_json::Value> =
        manifest.schedule.iter().filter_map(|task| serde_json::to_value(task).ok()).collect();
    let commands: Vec<serde_json::Value> =
        manifest.command.iter().filter_map(|command| serde_json::to_value(command).ok()).collect();
    let watches: Vec<serde_json::Value> =
        manifest.watch.iter().filter_map(|watch| serde_json::to_value(watch).ok()).collect();
    let event_listeners: Vec<serde_json::Value> = manifest
        .events
        .iter()
        .filter(|event| event.direction() == "listen")
        .map(|event| {
            serde_json::json!({
                "event": event.name,
                "function": event.handler,
                "queue": event.queue.clone().unwrap_or_else(|| "default".into()),
                "max_attempts": event.max_attempts.unwrap_or(3),
            })
        })
        .collect();
    let definition = manifest.plugin;
    let summary = |version: String,
                   artifact_path: Option<String>,
                   created: bool,
                   revision: Option<String>,
                   written_files: usize,
                   reused_files: usize| LoadedPlugin {
        name: definition.name.clone(),
        version,
        artifact_path,
        pages: pages.len(),
        public_pages: pages.iter().filter(|page| page.public).count(),
        theme: theme_summary.clone(),
        app: app_summary.clone(),
        created,
        revision,
        written_files,
        reused_files,
    };

    db.use_ns(namespace).await?;
    db.use_db(core_database).await?;

    // Everything this plugin already has in the catalog, newest first.
    let mut response = db
        .query("SELECT version, content_hash, revision, date_created FROM plugins WHERE name = $name ORDER BY date_created DESC;")
        .bind(("name", definition.name.clone()))
        .await?
        .check()?;
    let known: Vec<KnownVersion> = response.take(0)?;
    let same_version = |row: &&KnownVersion| base_version(&row.version) == definition.version;

    // A model id belongs to one plugin, in every version of it.
    if !models.is_empty() {
        let ids: Vec<String> = models.iter().filter_map(|model| model.model_id.clone()).collect();
        let mut response = db
            .query("SELECT model_id, plugin.name AS plugin FROM plugin_models WHERE model_id IN $ids;")
            .bind(("ids", ids))
            .await?
            .check()?;
        let taken: Vec<ModelIdOwner> = response.take(0)?;
        if let Some(other) = taken.into_iter().find(|owner| owner.plugin != definition.name) {
            let model = models
                .iter()
                .find(|model| model.model_id.as_deref() == Some(other.model_id.as_str()))
                .map_or_else(String::new, |model| model.name.clone());
            return Err(CatalogError::ModelIdTaken { model_id: other.model_id, model, plugin: other.plugin });
        }
    }

    // The same content again is not a new revision.
    if let Some(same) = known
        .iter()
        .filter(same_version)
        .find(|row| row.content_hash.as_deref() == Some(content_hash.as_str()))
    {
        let artifact_path = match &same.revision {
            Some(revision) => {
                let index = revisions::read_index(&layout, &definition.name, revision).await?;
                // Only repair `app_dir` if its copy has gone missing.
                if let Some(index) = &index {
                    if !revisions::missing_files(&layout, index).await?.is_empty() {
                        revisions::restore(&layout, index, &contents).await?;
                    }
                }
                artifact_logical
                    .as_ref()
                    .and_then(|logical| index.as_ref()?.files.get(logical).map(|entry| (logical, entry)))
                    .map(|(logical, entry)| revisions::relative_path(&definition.name, entry, logical))
                    .map(|path| path.to_string_lossy().into_owned())
            }
            // Stored in the older layout: files are in plugins/<name>/<version>/.
            None => artifact
                .as_ref()
                .map(|artifact| {
                    AppDir::plugin_relative_dir(&definition.name, &same.version)
                        .map(|dir| dir.join(&artifact.name).to_string_lossy().into_owned())
                })
                .transpose()?,
        };
        return Ok(summary(same.version.clone(), artifact_path, false, same.revision.clone(), 0, 0));
    }

    // New content. Compare its files by hash with the plugin's latest revision and write
    // only the ones that differ, into a new folder named after the date and time.
    let previous = match known.first() {
        Some(latest) => {
            let folder = latest.revision.clone().unwrap_or_else(|| latest.version.clone());
            revisions::read_index(&layout, &definition.name, &folder)
                .await?
                .map(|index| (folder, index))
        }
        None => None,
    };
    let stored = revisions::store(
        &layout,
        &definition.name,
        &definition.version,
        previous.as_ref().map(|(folder, index)| (folder.as_str(), index)),
        &contents,
    )
    .await?;

    // The first load of a version keeps the version as written; a rebuild of one that is
    // already catalogued is `<version>+<stamp>`, so nothing needs a manual version bump.
    let catalog_version = if known.iter().any(|row| base_version(&row.version) == definition.version) {
        format!("{}+{}", definition.version, stored.revision)
    } else {
        definition.version.clone()
    };
    let artifact_path = artifact_logical
        .as_ref()
        .and_then(|logical| stored.index.files.get(logical).map(|entry| (logical, entry)))
        .map(|(logical, entry)| {
            revisions::relative_path(&definition.name, entry, logical)
                .to_string_lossy()
                .into_owned()
        });
    write_view_files(&layout, &definition.name, &catalog_version, &pages).await?;

    let record = NewPlugin {
        name: definition.name.clone(),
        label: if definition.label.is_empty() {
            definition.name.clone()
        } else {
            definition.label.clone()
        },
        version: catalog_version.clone(),
        description: definition.description.clone(),
        long_description: definition.long_description.clone(),
        icon_path: definition.icon_path.clone(),
        website: definition.website.clone(),
        authors: definition
            .authors
            .iter()
            .map(|author| PluginDbAuthor {
                name: author.name.clone(),
                email: author.email.clone(),
                github: author.github.clone(),
                website: author.website.clone(),
            })
            .collect(),
        categories: definition
            .categories
            .iter()
            .map(|category| PluginDbCategory {
                name: category.name.clone(),
                label: category.label.clone(),
            })
            .collect(),
        dependencies: definition.dependencies.clone(),
        workspace: definition.workspace.clone(),
        kind: definition.kind.clone(),
        artifact_path: artifact_path.clone(),
        artifact_hash: wasm_hash,
        content_hash,
        revision: Some(stored.revision.clone()),
        source_path: Some(package_dir.to_string_lossy().into_owned()),
        app,
        public_functions: definition.public_functions.clone(),
        schedules,
        commands,
        i18n: text_catalogs.as_ref().map(|found| super::i18n::to_value(&found.catalogs)),
        watches,
        event_listeners,
        is_builtin: definition.is_builtin,
        is_active: true,
    };
    let model_rows: Vec<NewModelRow> = models
        .iter()
        .map(|model| NewModelRow {
            name: model.name.clone(),
            model_id: model.model_id.clone().unwrap_or_default(),
            label: model.label.clone(),
            definition: serde_json::to_value(model).unwrap_or(serde_json::Value::Null),
        })
        .collect();
    let rows: Vec<NewUiPage> = pages
        .iter()
        .map(|page| NewUiPage {
            layout: page.layout.clone(),
            route: page.route.clone(),
            is_pattern: page.is_pattern,
            route_shape: page.shape.clone(),
            title: page.title.clone(),
            model: page.model.clone(),
            models: page.models.iter().cloned().collect(),
            is_public: page.public,
            component_tree: page.tree.clone(),
            source_path: page.source.to_string_lossy().into_owned(),
        })
        .collect();
    db.query(
        r#"
        BEGIN TRANSACTION;
        LET $plugin = (CREATE ONLY plugins CONTENT $record).id;
        IF $theme != NONE {
            CREATE plugin_themes CONTENT {
                plugin: $plugin,
                name: $theme.name,
                label: $theme.label,
                is_system: $theme.is_system,
                color_mode: $theme.color_mode,
                tokens: $theme.tokens,
                layout: $theme.layout,
                error_pages: $theme.error_pages,
                nav: $theme.nav
            };
        };
        FOR $m IN $models {
            CREATE plugin_models CONTENT {
                plugin: $plugin,
                name: $m.name,
                table_name: $m.model_id,
                model_id: $m.model_id,
                label: $m.label,
                definition: $m.definition
            };
        };
        FOR $page IN $pages {
            CREATE plugin_ui_pages CONTENT {
                plugin: $plugin,
                route: $page.route,
                layout: $page.layout,
                is_pattern: $page.is_pattern,
                route_shape: $page.route_shape,
                title: $page.title,
                model: $page.model,
                models: $page.models,
                is_public: $page.is_public,
                component_tree: $page.component_tree,
                source_path: $page.source_path
            };
        };
        COMMIT TRANSACTION;
        "#,
    )
    .bind(("record", record))
    .bind(("theme", theme.map(|theme| NewTheme {
        name: theme.name,
        label: theme.label,
        is_system: is_system_theme,
        color_mode: theme.color_mode,
        tokens: theme.tokens,
        layout: theme.layout,
        error_pages: theme.error_pages,
        nav: theme.nav,
    })))
    .bind(("pages", rows))
    .bind(("models", model_rows))
    .await?
    .check()?;

    Ok(summary(
        catalog_version,
        artifact_path,
        true,
        Some(stored.revision),
        stored.written.len(),
        stored.reused.len(),
    ))
}

/// The launcher tile for a plugin that declares `[app]`. Its `route` must be a
/// literal page route of the plugin, so the tile always opens something.
fn read_app(
    manifest: &PluginManifest,
    manifest_path: &Path,
    pages: &[PageDocument],
) -> Result<Option<NewApp>, CatalogError> {
    let Some(app) = &manifest.app else {
        return Ok(None);
    };
    let route = super::pages::RoutePattern::parse(&app.route).map_err(|_| CatalogError::InvalidApp {
        path: manifest_path.to_path_buf(),
        field: "route",
        reason: "must be an absolute page route such as `/chat`",
    })?;
    if route.is_pattern() {
        return Err(CatalogError::InvalidApp {
            path: manifest_path.to_path_buf(),
            field: "route",
            reason: "cannot contain `{param}` segments: a tile opens one fixed page",
        });
    }
    let route = route.canonical();
    if !pages.iter().any(|page| page.route == route) {
        return Err(CatalogError::AppRouteNotAPage {
            path: manifest_path.to_path_buf(),
            route,
            pages: pages.iter().map(|page| page.route.as_str()).collect::<Vec<_>>().join(", "),
        });
    }
    if let Some(icon) = &app.icon
        && !super::themes::is_identifier(icon)
    {
        return Err(CatalogError::InvalidApp {
            path: manifest_path.to_path_buf(),
            field: "icon",
            reason: "must be a lucide icon name (lowercase letters, digits and `-`)",
        });
    }
    let label = app
        .label
        .clone()
        .filter(|label| !label.trim().is_empty())
        .or_else(|| (!manifest.plugin.label.trim().is_empty()).then(|| manifest.plugin.label.clone()))
        .unwrap_or_else(|| manifest.plugin.name.clone());
    Ok(Some(NewApp {
        label,
        icon: app.icon.clone(),
        route,
        description: app.description.clone().filter(|text| !text.trim().is_empty()),
    }))
}

/// The theme a plugin ships, read from `[theme].tokens_file`; `None` for a
/// plugin that is not a theme.
async fn read_theme(
    manifest: &PluginManifest,
    manifest_path: &Path,
    package_dir: &Path,
) -> Result<Option<ThemeDocument>, CatalogError> {
    let Some(theme) = &manifest.theme else {
        return Ok(None);
    };
    let tokens_file = theme
        .tokens_file
        .as_deref()
        .filter(|file| !file.is_empty())
        .ok_or_else(|| CatalogError::ThemeWithoutTokens(manifest_path.to_path_buf()))?;
    let relative = resolve_package_file(package_dir, tokens_file).await?;
    let path = package_dir.join(relative);
    let text = tokio::fs::read_to_string(&path).await.map_err(io_error(&path))?;
    Ok(Some(parse_theme(&text, &path, &theme.name, theme.label.as_deref())?))
}

/// Every model a page names must be one the plugin defines in `models/`. A page may write the
/// model as `workspace.name`; only the part after the last `.` is the model name.
fn validate_page_models(
    manifest: &PluginManifest,
    models: &[ModelDef],
    pages: &[PageDocument],
) -> Result<(), CatalogError> {
    let _ = manifest;
    let declared: BTreeSet<&str> = models.iter().map(|model| model.name.as_str()).collect();
    for page in pages {
        for model in &page.models {
            if !declared.contains(model_name(model)) {
                return Err(CatalogError::UnknownPageModel {
                    page: page.source.clone(),
                    model: model.clone(),
                });
            }
        }
    }
    Ok(())
}

/// The models a plugin is granted access to must exist.
fn validate_granted_models(
    manifest: &PluginManifest,
    models: &[ModelDef],
    manifest_path: &Path,
) -> Result<(), CatalogError> {
    let declared: BTreeSet<&str> = models.iter().map(|model| model.name.as_str()).collect();
    let plugin = &manifest.plugin;
    for (list, entries) in [("access_models", &plugin.access_models), ("public_access_models", &plugin.public_access_models)] {
        if let Some(entry) = entries.iter().find(|entry| !declared.contains(entry.name.as_str())) {
            return Err(CatalogError::UnknownGrantedModel {
                path: manifest_path.to_path_buf(),
                name: entry.name.clone(),
                list,
            });
        }
    }
    Ok(())
}

/// `base.currency` -> `currency`.
pub fn model_name(reference: &str) -> &str {
    reference.rsplit('.').next().unwrap_or(reference)
}

/// Package-relative files, beyond `plugin.toml` and the WASM artifact, that
/// the manifest points at.
fn declared_files(manifest: &PluginManifest) -> Vec<&str> {
    manifest
        .theme
        .iter()
        .filter_map(|theme| theme.tokens_file.as_deref())
        .filter(|file| !file.is_empty())
        .collect()
}

/// Resolve a manifest-declared file to a path relative to `package_dir`,
/// requiring that it exists, is a regular file, and stays inside the package.
async fn resolve_package_file(package_dir: &Path, declared: &str) -> Result<PathBuf, CatalogError> {
    let candidate = package_dir.join(declared);
    let canonical = tokio::fs::canonicalize(&candidate)
        .await
        .map_err(io_error(&candidate))?;
    let relative = canonical
        .strip_prefix(package_dir)
        .map_err(|_| CatalogError::FileOutsidePackage {
            path: canonical.clone(),
            package: package_dir.to_path_buf(),
        })?;
    if !canonical.is_file() {
        return Err(CatalogError::FileOutsidePackage {
            path: canonical,
            package: package_dir.to_path_buf(),
        });
    }
    Ok(relative.to_path_buf())
}

/// The WASM artifact: where it is in the package, and the file name it gets
/// at the root of the plugin's `app_dir` folder.
struct Artifact {
    source: PathBuf,
    name: PathBuf,
}

/// A theme added to an organization by installing its plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledTheme {
    pub name: String,
    pub layout: String,
    /// `true` when it became the organization's active theme (the first theme
    /// an organization installs does).
    pub activated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
struct PlannedTheme {
    plugin: surrealdb::types::RecordId,
    name: String,
    label: String,
    is_system: bool,
    color_mode: String,
    tokens: serde_json::Value,
    layout: String,
    error_pages: String,
    nav: Option<serde_json::Value>,
}

/// Result of [`install_plugins`], in installation order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstallReport {
    /// `(name, version)` pairs newly recorded for the organization.
    pub installed: Vec<(String, String)>,
    /// `(name, version)` pairs the organization already had.
    pub already_installed: Vec<(String, String)>,
    /// Themes that came with the installed plugins.
    pub themes: Vec<InstalledTheme>,
}

/// Install catalog plugins (and missing dependencies, dependencies first) for
/// the organization whose database is `org_database`.
pub async fn install_plugins(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    org_database: &str,
    specs: &[PluginSpec],
) -> Result<InstallReport, CatalogError> {
    db.use_ns(namespace).await?;
    db.use_db(core_database).await?;
    let mut response = db
        .query("SELECT db_name FROM org_databases WHERE db_name = $org LIMIT 1;")
        .bind(("org", org_database.to_string()))
        .await?
        .check()?;
    let orgs: Vec<OrgDatabaseRow> = response.take(0)?;
    if orgs.is_empty() {
        return Err(CatalogError::OrganizationNotFound(org_database.to_string()));
    }

    db.use_db(org_database).await?;
    let mut response = db
        .query("SELECT plugin_name, version FROM installed_plugins;")
        .await?
        .check()?;
    let installed_rows: Vec<InstalledRow> = response.take(0)?;
    let installed: HashMap<String, String> = installed_rows
        .into_iter()
        .map(|row| (row.plugin_name, row.version))
        .collect();

    db.use_db(core_database).await?;
    let mut plan = Plan {
        installed: &installed,
        order: Vec::new(),
        visiting: HashSet::new(),
        visited: HashSet::new(),
        already_installed: Vec::new(),
    };
    for spec in specs {
        plan.visit(db, spec, true).await?;
    }
    let Plan {
        order,
        already_installed,
        ..
    } = plan;
    check_route_conflicts(db, &installed, &order).await?;
    let themes = fetch_planned_themes(db, &order).await?;
    let planned_models = fetch_planned_models(db, &order).await?;

    db.use_db(org_database).await?;
    // Work out what every model needs before changing anything: one that cannot be applied
    // (a new required field with no default over existing records, ...) stops the install.
    let mut model_plans = Vec::with_capacity(order.len());
    for record in &order {
        let defs = models_of(&planned_models, &record.id);
        let plans = model_apply::plan_models(db, &record.name, &defs).await?;
        if let Some((model, problems)) = model_apply::blockers(&plans).into_iter().next() {
            return Err(model_apply::ApplyError::Blocked { model, problems }.into());
        }
        model_plans.push(plans);
    }
    let mut report = InstallReport {
        installed: Vec::with_capacity(order.len()),
        already_installed,
        themes: Vec::new(),
    };
    let mut has_active_theme = {
        let mut response = db
            .query("SELECT VALUE id FROM ui_theme_config LIMIT 1;")
            .await?
            .check()?;
        let configured: Vec<surrealdb::types::RecordId> = response.take(0)?;
        !configured.is_empty()
    };
    for (index, record) in order.into_iter().enumerate() {
        let dependencies = record.dependencies.clone().unwrap_or_default();
        let theme = themes.iter().find(|theme| theme.plugin == record.id).cloned();
        model_apply::apply_plans(db, &model_plans[index]).await?;
        db.query(
            r#"
            BEGIN TRANSACTION;
            CREATE type::record('installed_plugins', $name) SET
                plugin_name = $name,
                version = $version,
                is_enabled = true,
                installed_by = $installed_by;
            LET $from = type::record('installed_plugins', $name);
            FOR $dependency IN $dependencies {
                LET $to = type::record('installed_plugins', $dependency);
                RELATE $from->installed_plugin_depends_on->$to
                    SET requirement = 'required';
            };
            IF $theme != NONE {
                UPSERT type::record('ui_themes', $theme.name) CONTENT {
                    name: $theme.name,
                    label: $theme.label,
                    is_system: $theme.is_system,
                    is_active: true,
                    color_mode: $theme.color_mode,
                    tokens: $theme.tokens,
                    layout: $theme.layout,
                    error_pages: $theme.error_pages,
                    nav: $theme.nav,
                    plugin_name: $name,
                    plugin_version: $version
                };
                LET $configured = (SELECT VALUE id FROM ui_theme_config LIMIT 1);
                IF array::len($configured) = 0 {
                    CREATE ui_theme_config:org SET
                        active_theme = type::record('ui_themes', $theme.name),
                        color_mode = $theme.color_mode;
                };
            };
            COMMIT TRANSACTION;
            "#,
        )
        .bind(("theme", theme.clone()))
        .bind(("name", record.name.clone()))
        .bind(("version", record.version.clone()))
        .bind(("installed_by", INSTALLED_BY_CLI.to_string()))
        .bind(("dependencies", dependencies))
        .await?
        .check()?;
        sync_schedules(db, &record.name, record.schedules.as_deref()).await?;
        crate::plugin_files::watch::sync_watches(db, &record.name, record.watches.as_deref()).await?;
        crate::plugin_events::sync_listeners(db, &record.name, record.event_listeners.as_deref()).await?;
        if let Some(theme) = theme {
            report.themes.push(InstalledTheme {
                name: theme.name,
                layout: theme.layout,
                activated: !has_active_theme,
            });
            has_active_theme = true;
        }
        report.installed.push((record.name, record.version));
    }

    Ok(report)
}

/// Result of [`upgrade_plugins`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpgradeReport {
    /// `(name, from, to)` for each plugin moved to another catalog version.
    pub upgraded: Vec<(String, String, String)>,
    /// `(name, version)` for plugins already on the version asked for.
    pub already_current: Vec<(String, String)>,
}

/// Move an organization to another catalog version of plugins it has installed: the newest
/// loaded one, or the one named (`name@version`). Nothing is copied: catalog versions are
/// already on disk, as revisions that share their unchanged files.
///
/// The target's dependencies must already be installed, and its pages must not take over a
/// route another installed plugin serves. A theme the plugin ships is updated in place and
/// stays active if it was.
pub async fn upgrade_plugins(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    org_database: &str,
    specs: &[PluginSpec],
) -> Result<UpgradeReport, CatalogError> {
    db.use_ns(namespace).await?;
    db.use_db(core_database).await?;
    let mut response = db
        .query("SELECT db_name FROM org_databases WHERE db_name = $org LIMIT 1;")
        .bind(("org", org_database.to_string()))
        .await?
        .check()?;
    let orgs: Vec<OrgDatabaseRow> = response.take(0)?;
    if orgs.is_empty() {
        return Err(CatalogError::OrganizationNotFound(org_database.to_string()));
    }

    db.use_db(org_database).await?;
    let mut response = db
        .query("SELECT plugin_name, version FROM installed_plugins;")
        .await?
        .check()?;
    let rows: Vec<InstalledRow> = response.take(0)?;
    let mut installed: HashMap<String, String> =
        rows.into_iter().map(|row| (row.plugin_name, row.version)).collect();

    let mut report = UpgradeReport::default();
    for spec in specs {
        let Some(from) = installed.get(&spec.name).cloned() else {
            return Err(CatalogError::NotInstalled(spec.name.clone()));
        };
        db.use_db(core_database).await?;
        let target = select_catalog_version(db, spec).await?;
        if target.version == from {
            report.already_current.push((spec.name.clone(), from));
            continue;
        }
        for dependency in target.dependencies.clone().unwrap_or_default() {
            if !installed.contains_key(&dependency) {
                return Err(CatalogError::MissingDependency {
                    plugin: spec.name.clone(),
                    dependency,
                });
            }
        }
        // The plugin's own old pages do not count against its new ones.
        let others: HashMap<String, String> = installed
            .iter()
            .filter(|(name, _)| **name != spec.name)
            .map(|(name, version)| (name.clone(), version.clone()))
            .collect();
        check_route_conflicts(db, &others, std::slice::from_ref(&target)).await?;
        let theme = fetch_planned_themes(db, std::slice::from_ref(&target))
            .await?
            .into_iter()
            .next();
        let planned_models = fetch_planned_models(db, std::slice::from_ref(&target)).await?;

        db.use_db(org_database).await?;
        // Make the organization's data match the new version's models first. A change that
        // cannot be applied safely stops the upgrade before the version is moved.
        let defs = models_of(&planned_models, &target.id);
        let plans = model_apply::plan_models(db, &spec.name, &defs).await?;
        model_apply::apply_plans(db, &plans).await?;
        db.query(
            r#"
            BEGIN TRANSACTION;
            UPDATE type::record('installed_plugins', $name) SET version = $version;
            IF $theme != NONE {
                UPSERT type::record('ui_themes', $theme.name) MERGE {
                    name: $theme.name,
                    label: $theme.label,
                    is_system: $theme.is_system,
                    color_mode: $theme.color_mode,
                    tokens: $theme.tokens,
                    layout: $theme.layout,
                    error_pages: $theme.error_pages,
                    nav: $theme.nav,
                    plugin_name: $name,
                    plugin_version: $version
                };
            };
            COMMIT TRANSACTION;
            "#,
        )
        .bind(("theme", theme))
        .bind(("name", spec.name.clone()))
        .bind(("version", target.version.clone()))
        .await?
        .check()?;
        sync_schedules(db, &spec.name, target.schedules.as_deref()).await?;
        crate::plugin_files::watch::sync_watches(db, &spec.name, target.watches.as_deref()).await?;
        crate::plugin_events::sync_listeners(db, &spec.name, target.event_listeners.as_deref()).await?;
        installed.insert(spec.name.clone(), target.version.clone());
        report.upgraded.push((spec.name.clone(), from, target.version));
    }
    Ok(report)
}

/// Links to another plugin's model (`"target": "currency.currency"`): the plugin must list that plugin
/// as a dependency, and the catalog must have the model under the `target_id` written in the field,
/// so a typo or a model that moved is found when the plugin is loaded, not when it is used.
async fn check_foreign_links(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    plugin: &super::models::plugin_def::PluginDefinition,
    models: &[ModelDef],
) -> Result<(), CatalogError> {
    let mut wanted: Vec<(String, String, String, String)> = Vec::new();
    for model in models {
        for field in model.live_fields() {
            let Some(target) = field.target.as_deref() else { continue };
            let Some((other_plugin, other_model)) = crate::data_model::foreign_target(target) else { continue };
            if other_plugin != plugin.name && !plugin.dependencies.iter().any(|dependency| dependency == other_plugin) {
                return Err(CatalogError::ForeignLink(format!(
                    "{}.{} links to `{target}`, so `{other_plugin}` must be listed under `dependencies` in plugin.toml",
                    model.name, field.name
                )));
            }
            wanted.push((
                format!("{}.{}", model.name, field.name),
                other_plugin.to_string(),
                other_model.to_string(),
                field.target_id.clone().unwrap_or_default(),
            ));
        }
    }
    if wanted.is_empty() {
        return Ok(());
    }
    db.use_ns(namespace).await?;
    db.use_db(core_database).await?;
    for (field, other_plugin, other_model, id) in wanted {
        let mut response = db
            .query(
                "SELECT VALUE model_id FROM plugin_models WHERE name = $model \
                 AND plugin IN (SELECT VALUE id FROM plugins WHERE name = $plugin AND is_active = true);",
            )
            .bind(("model", other_model.clone()))
            .bind(("plugin", other_plugin.clone()))
            .await?
            .check()?;
        let ids: Vec<Option<String>> = response.take(0)?;
        if ids.is_empty() {
            return Err(CatalogError::ForeignLink(format!(
                "{field} links to `{other_plugin}.{other_model}`, but the catalog has no such model: load `{other_plugin}` first (`aether --load-plugin`)"
            )));
        }
        if !ids.iter().flatten().any(|known| known == &id) {
            return Err(CatalogError::ForeignLink(format!(
                "{field}: `target_id` `{id}` is not the id of `{other_plugin}.{other_model}`; run `aether --sync-models` again"
            )));
        }
    }
    Ok(())
}

/// The ids of other plugins' models, by `plugin.model`, for `aether --sync-models` to write into links.
/// A name the catalog does not have is simply missing from the answer.
pub async fn foreign_model_ids(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    names: &std::collections::BTreeSet<String>,
) -> Result<HashMap<String, String>, CatalogError> {
    db.use_ns(namespace).await?;
    db.use_db(core_database).await?;
    let mut found = HashMap::new();
    for name in names {
        let Some((plugin, model)) = crate::data_model::foreign_target(name) else { continue };
        let mut response = db
            .query(
                "SELECT VALUE model_id FROM plugin_models WHERE name = $model \
                 AND plugin IN (SELECT VALUE id FROM plugins WHERE name = $plugin AND is_active = true) LIMIT 1;",
            )
            .bind(("model", model.to_string()))
            .bind(("plugin", plugin.to_string()))
            .await?
            .check()?;
        let ids: Vec<Option<String>> = response.take(0)?;
        if let Some(Some(id)) = ids.into_iter().next() {
            found.insert(name.clone(), id);
        }
    }
    Ok(found)
}

/// Make an organization's recurring tasks for `plugin` match the catalog version's `[[schedule]]`.
/// Caller must be on the organization's database.
async fn sync_schedules(
    db: &Surreal<Client>,
    plugin: &str,
    schedules: Option<&[serde_json::Value]>,
) -> Result<(), CatalogError> {
    let defs: Vec<crate::scheduler::TaskDef> = schedules
        .unwrap_or_default()
        .iter()
        .filter_map(|value| serde_json::from_value(value.clone()).ok())
        .collect();
    crate::scheduler::tasks::sync_manifest(db, plugin, &defs)
        .await
        .map_err(|error| CatalogError::Schedule(plugin.to_string(), error.to_string()))
}

#[derive(Debug, Deserialize, SurrealValue)]
struct PlannedModel {
    plugin: surrealdb::types::RecordId,
    definition: serde_json::Value,
}

/// The models of the plugin versions about to be installed. Caller must be on the core database.
async fn fetch_planned_models(
    db: &Surreal<Client>,
    order: &[PluginDbDefinition],
) -> Result<Vec<(surrealdb::types::RecordId, ModelDef)>, CatalogError> {
    if order.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<_> = order.iter().map(|record| record.id.clone()).collect();
    let mut response = db
        .query("SELECT plugin, definition, name FROM plugin_models WHERE plugin IN $ids ORDER BY name;")
        .bind(("ids", ids))
        .await?
        .check()?;
    let rows: Vec<PlannedModel> = response.take(0)?;
    rows.into_iter()
        .map(|row| {
            serde_json::from_value::<ModelDef>(row.definition)
                .map(|model| (row.plugin, model))
                .map_err(|source| CatalogError::ModelFile(ModelFileError::Parse { path: PathBuf::from("plugin_models"), source }))
        })
        .collect()
}

fn models_of(all: &[(surrealdb::types::RecordId, ModelDef)], plugin: &surrealdb::types::RecordId) -> Vec<ModelDef> {
    all.iter().filter(|(id, _)| id == plugin).map(|(_, model)| model.clone()).collect()
}

/// The themes shipped by the plugins about to be installed. Caller must be on
/// the core database.
async fn fetch_planned_themes(
    db: &Surreal<Client>,
    order: &[PluginDbDefinition],
) -> Result<Vec<PlannedTheme>, CatalogError> {
    if order.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<_> = order.iter().map(|record| record.id.clone()).collect();
    let mut response = db
        .query(
            "SELECT plugin, name, label, is_system, color_mode, tokens, layout, error_pages, nav \
             FROM plugin_themes WHERE plugin IN $ids;",
        )
        .bind(("ids", ids))
        .await?
        .check()?;
    Ok(response.take(0)?)
}

#[derive(Debug, Deserialize, SurrealValue)]
struct ThemeColorMode {
    color_mode: String,
}

/// Make an installed theme the organization's active one.
pub async fn activate_theme(
    db: &Surreal<Client>,
    namespace: &str,
    org_database: &str,
    theme_name: &str,
) -> Result<(), CatalogError> {
    db.use_ns(namespace).await?;
    db.use_db(org_database).await?;
    let mut response = db
        .query("SELECT color_mode FROM ui_themes WHERE name = $name LIMIT 1;")
        .bind(("name", theme_name.to_string()))
        .await?
        .check()?;
    let found: Vec<ThemeColorMode> = response.take(0)?;
    let Some(theme) = found.into_iter().next() else {
        return Err(CatalogError::ThemeNotInstalled(theme_name.to_string()));
    };
    db.query(
        "UPSERT ui_theme_config:org SET active_theme = type::record('ui_themes', $name), color_mode = $mode;",
    )
    .bind(("name", theme_name.to_string()))
    .bind(("mode", theme.color_mode))
    .await?
    .check()?;
    Ok(())
}

#[derive(Debug, Deserialize, SurrealValue)]
struct PlannedPageRoute {
    route: String,
    route_shape: String,
    plugin: surrealdb::types::RecordId,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct ClaimedRoute {
    route: String,
    route_shape: String,
    plugin_name: String,
    plugin_version: String,
}

/// Fail when a page route of a plugin about to be installed would match the
/// same URLs as a route already served in the organization (by an installed
/// plugin, or by another plugin in the same install). Routes conflict when
/// they have the same shape: `/chat/{channel}` and `/chat/{room}` do,
/// `/chat/new` and `/chat/{channel}` do not (the literal wins). Caller must be
/// on the core database.
async fn check_route_conflicts(
    db: &Surreal<Client>,
    installed: &HashMap<String, String>,
    order: &[PluginDbDefinition],
) -> Result<(), CatalogError> {
    if order.is_empty() {
        return Ok(());
    }
    let ids: Vec<_> = order.iter().map(|record| record.id.clone()).collect();
    let mut response = db
        .query("SELECT route, route_shape, plugin FROM plugin_ui_pages WHERE plugin IN $ids;")
        .bind(("ids", ids))
        .await?
        .check()?;
    let planned: Vec<PlannedPageRoute> = response.take(0)?;
    if planned.is_empty() {
        return Ok(());
    }

    let name_of = |id: &surrealdb::types::RecordId| {
        order
            .iter()
            .find(|record| &record.id == id)
            .map(|record| record.name.clone())
            .unwrap_or_default()
    };

    let mut owners: HashMap<&str, String> = HashMap::new();
    for page in &planned {
        let owner = name_of(&page.plugin);
        match owners.get(page.route_shape.as_str()) {
            Some(existing) if *existing != owner => {
                return Err(CatalogError::RouteConflict {
                    route: page.route.clone(),
                    existing: existing.clone(),
                    plugin: owner,
                });
            }
            _ => {
                owners.insert(page.route_shape.as_str(), owner);
            }
        }
    }

    let shapes: Vec<String> = owners.keys().map(|shape| shape.to_string()).collect();
    let mut response = db
        .query(
            "SELECT route, route_shape, plugin.name AS plugin_name, plugin.version AS plugin_version \
             FROM plugin_ui_pages WHERE route_shape IN $shapes;",
        )
        .bind(("shapes", shapes))
        .await?
        .check()?;
    let claimed: Vec<ClaimedRoute> = response.take(0)?;
    for claim in claimed {
        let serves_this_org = installed
            .get(&claim.plugin_name)
            .is_some_and(|version| *version == claim.plugin_version);
        if serves_this_org && let Some(owner) = owners.get(claim.route_shape.as_str()) {
            return Err(CatalogError::RouteConflict {
                route: claim.route,
                existing: claim.plugin_name,
                plugin: owner.clone(),
            });
        }
    }
    Ok(())
}

/// Dependency-ordered install plan, resolved against the core catalog.
struct Plan<'a> {
    installed: &'a HashMap<String, String>,
    order: Vec<PluginDbDefinition>,
    visiting: HashSet<String>,
    visited: HashSet<String>,
    already_installed: Vec<(String, String)>,
}

impl Plan<'_> {
    /// Post-order DFS so dependencies precede their dependents. `requested`
    /// specs must match an existing install exactly; dependencies that are
    /// already installed are accepted at whatever version the org has.
    async fn visit(
        &mut self,
        db: &Surreal<Client>,
        spec: &PluginSpec,
        requested: bool,
    ) -> Result<(), CatalogError> {
        if self.visited.contains(&spec.name) {
            return Ok(());
        }
        if !self.visiting.insert(spec.name.clone()) {
            return Err(CatalogError::DependencyCycle(spec.name.clone()));
        }

        if let Some(installed_version) = self.installed.get(&spec.name) {
            if requested
                && let Some(wanted) = &spec.version
                && wanted != installed_version
            {
                return Err(CatalogError::VersionMismatch {
                    name: spec.name.clone(),
                    installed: installed_version.clone(),
                    requested: wanted.clone(),
                });
            }
            self.already_installed
                .push((spec.name.clone(), installed_version.clone()));
        } else {
            let record = select_catalog_version(db, spec).await?;
            for dependency in record.dependencies.clone().unwrap_or_default() {
                let dependency = PluginSpec {
                    name: dependency,
                    version: None,
                };
                Box::pin(self.visit(db, &dependency, false)).await?;
            }
            self.order.push(record);
        }

        self.visiting.remove(&spec.name);
        self.visited.insert(spec.name.clone());
        Ok(())
    }
}

/// Pick the active catalog row for `spec`. Without an explicit version, the one loaded
/// most recently.
async fn select_catalog_version(
    db: &Surreal<Client>,
    spec: &PluginSpec,
) -> Result<PluginDbDefinition, CatalogError> {
    let mut response = db
        .query("SELECT * FROM plugins WHERE name = $name AND is_active = true ORDER BY date_created DESC;")
        .bind(("name", spec.name.clone()))
        .await?
        .check()?;
    let rows: Vec<PluginDbDefinition> = response.take(0)?;

    match &spec.version {
        Some(version) => rows
            .into_iter()
            .find(|row| &row.version == version)
            .ok_or_else(|| CatalogError::VersionNotFound {
                name: spec.name.clone(),
                version: version.clone(),
            }),
        None => rows
            .into_iter()
            .next()
            .ok_or_else(|| CatalogError::PluginNotFound(spec.name.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_versioned_specs() -> Result<(), CatalogError> {
        let plain: PluginSpec = "partner".parse()?;
        assert_eq!(plain.name, "partner");
        assert_eq!(plain.version, None);

        let pinned: PluginSpec = "partner@0.1.0".parse()?;
        assert_eq!(pinned.version.as_deref(), Some("0.1.0"));
        Ok(())
    }

    #[test]
    fn rejects_malformed_specs() {
        for raw in ["", "@1.0.0", "partner@", " @ "] {
            assert!(
                matches!(raw.parse::<PluginSpec>(), Err(CatalogError::InvalidSpec(_))),
                "`{raw}` should be rejected"
            );
        }
    }

    #[test]
    fn hashes_to_lowercase_hex() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[tokio::test]
    async fn rejects_declared_files_outside_the_package() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = tempfile::tempdir()?;
        let package = tokio::fs::canonicalize(root.path()).await?.join("pkg");
        tokio::fs::create_dir_all(&package).await?;
        tokio::fs::write(root.path().join("secret.txt"), "s").await?;

        let result = resolve_package_file(&package, "../secret.txt").await;
        assert!(matches!(result, Err(CatalogError::FileOutsidePackage { .. })));
        Ok(())
    }

    fn manifest_with_app(app: &str) -> Result<PluginManifest, ManifestError> {
        PluginManifest::parse(&format!(
            "[plugin]\nname = \"chat\"\nlabel = \"Team Chat\"\nversion = \"1\"\n{app}"
        ))
    }

    fn page(route: &str) -> Result<PageDocument, PageError> {
        super::super::pages::parse_page(
            &format!("<page route=\"{route}\" />"),
            Path::new("pages/p.xml"),
        )
    }

    #[test]
    fn an_app_must_open_a_page_of_the_plugin() -> Result<(), Box<dyn std::error::Error>> {
        let pages = [page("/chat")?];
        let manifest = manifest_with_app("[app]\nroute = \"/chat\"\nicon = \"message-square\"\n")?;
        let app = read_app(&manifest, Path::new("plugin.toml"), &pages)?.ok_or("no app")?;
        assert_eq!((app.route.as_str(), app.label.as_str()), ("/chat", "Team Chat"));
        assert_eq!(app.icon.as_deref(), Some("message-square"));

        let elsewhere = manifest_with_app("[app]\nroute = \"/other\"\n")?;
        assert!(matches!(
            read_app(&elsewhere, Path::new("plugin.toml"), &pages),
            Err(CatalogError::AppRouteNotAPage { .. })
        ));
        Ok(())
    }

    #[test]
    fn an_app_route_is_a_fixed_page_and_the_icon_a_plain_name() -> Result<(), Box<dyn std::error::Error>> {
        let pages = [page("/chat/{channel}")?];
        for app in [
            "[app]\nroute = \"/chat/{channel}\"\n",
            "[app]\nroute = \"chat\"\n",
            "[app]\nroute = \"/chat\"\nicon = \"Not An Icon\"\n",
        ] {
            let manifest = manifest_with_app(app)?;
            assert!(
                read_app(&manifest, Path::new("plugin.toml"), &[page("/chat")?, pages[0].clone()]).is_err(),
                "{app}"
            );
        }
        // No `[app]`: not an app.
        let plain = manifest_with_app("")?;
        assert!(read_app(&plain, Path::new("plugin.toml"), &pages)?.is_none());
        Ok(())
    }

    #[test]
    fn the_app_label_can_be_overridden_and_defaults_to_the_plugin() -> Result<(), Box<dyn std::error::Error>> {
        let pages = [page("/chat")?];
        let named = manifest_with_app("[app]\nlabel = \"Chat\"\nroute = \"/chat\"\n")?;
        assert_eq!(
            read_app(&named, Path::new("plugin.toml"), &pages)?.map(|app| app.label),
            Some("Chat".to_string())
        );
        Ok(())
    }
}
