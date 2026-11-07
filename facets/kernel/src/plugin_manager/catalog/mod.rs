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
use aether_security::capabilities::CapabilityCatalog;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use surrealdb::{Surreal, engine::remote::ws::Client, types::SurrealValue};
use thiserror::Error;
use super::components::{
    ComponentDocument, ComponentError, ComponentRef, ComponentRegistry, collect_uses,
    discover_components, expand_tree, registry_from_documents,
};
use super::models::plugin_db_def::{PluginDbAuthor, PluginDbCategory, PluginDbDefinition};
use super::models::plugin_def::{ManifestError, PluginManifest};
use super::pages::{PageDocument, PageError, discover_pages, write_view_files};
use super::revisions;
use super::shell_defs::{
    ErrorSetDef, LayoutDef, ShellDefError, discover_error_sets, discover_layouts,
    error_sets_by_name, is_builtin_error_set, is_builtin_layout, layouts_by_name,
};
use super::themes::{ThemeDocument, ThemeError, parse_theme};
use crate::app_dir::{AppDir, AppDirError};
use crate::data_model::{ModelDef, ModelFileError, apply as model_apply, read_models};

mod install;
mod load;
mod package;
mod read;
mod rows;
mod settings;
mod stage;
mod validate;

#[allow(unused_imports)]
pub use self::{install::*, load::*, package::*, read::*, rows::*, validate::*};

const MANIFEST_FILE: &str = "plugin.toml";

const INSTALLED_BY_CLI: &str = "cli";

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("database error: {0}")]
    Database(#[from] surrealdb::Error),

    #[error("{0}")]
    ForeignLink(String),

    #[error("the rules of plugin `{plugin}` are wrong: {problems}")]
    Rules { plugin: String, problems: String },

    #[error("{0}")]
    Settings(String),

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

    #[error(transparent)]
    Component(#[from] ComponentError),

    #[error("i18n: {0}")]
    I18n(String),

    #[error(transparent)]
    Theme(#[from] ThemeError),

    #[error(transparent)]
    ShellDef(#[from] ShellDefError),

    #[error(
        "{path}: theme `{field}` is `{name}`, which is neither a built-in nor defined under `{folder}/`"
    )]
    UnknownShellDef {
        path: PathBuf,
        field: &'static str,
        name: String,
        folder: &'static str,
    },

    #[error("{path}: `[app].route` is `{route}`, which is not a page of this plugin. {fix}")]
    AppRouteNotAPage {
        path: PathBuf,
        route: String,
        /// What to change, worded for the person who wrote the plugin.
        fix: String,
    },

    #[error("{path}: `[app].{field}` is invalid: {reason}")]
    InvalidApp {
        path: PathBuf,
        field: &'static str,
        reason: &'static str,
    },

    #[error("{path}: the stylesheet of theme {theme} {reason}")]
    ThemeStylesheet {
        path: PathBuf,
        theme: PathBuf,
        reason: String,
    },

    #[error("{path}: the icon sprite of theme {theme} {reason}")]
    ThemeIcons {
        path: PathBuf,
        theme: PathBuf,
        reason: String,
    },

    #[error("{0}: `[theme]` needs `tokens_file`")]
    ThemeWithoutTokens(PathBuf),

    #[error("theme `{0}` is not installed in this organization")]
    ThemeNotInstalled(String),

    #[error("{page}: page uses model `{model}`, which the plugin does not declare")]
    UnknownPageModel { page: PathBuf, model: String },

    #[error("{path}: uses model `{model}`, which the plugin does not declare")]
    UnknownComponentModel { path: PathBuf, model: String },

    #[error("{path}: `{reference}`: {reason}")]
    BadComponentUse {
        path: PathBuf,
        reference: String,
        reason: String,
    },

    #[error(
        "upgrading `{plugin}` would remove component `{component}`, which `{consumer}` still uses"
    )]
    ComponentStillUsed {
        plugin: String,
        component: String,
        consumer: String,
    },

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

    #[error("the documentation of plugin `{plugin}` ([docs] in plugin.toml) has problems: {problems}")]
    Docs { plugin: String, problems: String },

    #[error("manifest {0} does not declare `plugin.wasm_file` or `plugin.script`")]
    MissingWasmFile(PathBuf),

    #[error("manifest {0} declares both `plugin.wasm_file` and `plugin.script`; a plugin has one")]
    BothWasmAndScript(PathBuf),

    #[error("script {path} does not compile: {message}")]
    ScriptInvalid { path: PathBuf, message: String },

    #[error("script {path} must end in `.rhai` or `.lua`")]
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

    #[error("invalid plugin spec `{0}`; expected `name`, `name@version`, or `all`")]
    InvalidSpec(String),

    #[error("{path}: {source}; capabilities are listed in `capabilities/`")]
    UnknownCapability {
        path: PathBuf,
        #[source]
        source: aether_security::capabilities::CapabilityError,
    },

    #[error(
        "{path}: `public_capabilities` lists `{capability}`, which the plugin does not list in `capabilities`"
    )]
    PublicCapabilityNotHeld { path: PathBuf, capability: String },

    #[error("the built-in capability catalog is invalid: {0}")]
    CapabilityCatalog(String),

    #[error(transparent)]
    Revision(#[from] super::revisions::RevisionError),

    #[error(transparent)]
    ModelFile(#[from] ModelFileError),

    #[error(transparent)]
    ModelSchema(#[from] model_apply::ApplyError),

    #[error(
        "{path}: `{name}` is listed in `{list}` but there is no model of that name; define it in `models/{name}.json`"
    )]
    UnknownGrantedModel {
        path: PathBuf,
        name: String,
        list: &'static str,
    },

    #[error(
        "model id `{model_id}` ({model}) is already used by plugin `{plugin}`; ids must be unique across plugins"
    )]
    ModelIdTaken {
        model_id: String,
        model: String,
        plugin: String,
    },

    #[error("organization database `{0}` was not found")]
    OrganizationNotFound(String),

    #[error("plugin `{0}` is not in the catalog")]
    PluginNotFound(String),

    /// A dependency walked during install is not loaded (`aether --load-plugins`).
    #[error(
        "plugin `{dependency}` is not in the catalog (required by `{required_by}`); \
         load it with `aether --load-plugins` first"
    )]
    DependencyNotInCatalog {
        dependency: String,
        required_by: String,
    },

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

pub fn base_version(version: &str) -> &str {
    version.split('+').next().unwrap_or(version)
}

/// A package-relative path as it is written in `files.json`.
fn logical_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_themes_stylesheet_is_checked_and_kept_with_the_theme()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let package = dir.path().canonicalize()?;
        std::fs::write(
            package.join("theme.css"),
            "[data-part=\"list.row\"] { background: url(assets/row.png); }",
        )?;
        let json = r##"{ "light": {"background": "#fff"}, "dark": {"background": "#000"},
                         "css": "theme.css" }"##;
        let theme_path = package.join("theme.json");
        let mut theme = parse_theme(json, &theme_path, "sea", None)?;
        attach_stylesheet(&mut theme, &package, "sea_theme", &theme_path).await?;
        let shell = theme.shell.ok_or("no shell")?;
        assert!(shell.get("css_file").is_none());
        assert!(
            shell["css"]
                .as_str()
                .is_some_and(|css| css.contains("/api/ui/plugins/sea_theme/assets/row.png"))
        );

        // A stylesheet that reaches out refuses the whole plugin.
        std::fs::write(
            package.join("theme.css"),
            "@import url(https://evil.example/x.css);",
        )?;
        let mut bad = parse_theme(json, &theme_path, "sea", None)?;
        let refused = attach_stylesheet(&mut bad, &package, "sea_theme", &theme_path).await;
        assert!(matches!(refused, Err(CatalogError::ThemeStylesheet { .. })));
        Ok(())
    }

    #[tokio::test]
    async fn a_themes_icon_sprite_is_checked_and_kept_with_the_theme()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let package = dir.path().canonicalize()?;
        std::fs::write(
            package.join("icons.svg"),
            r#"<svg><symbol id="bell" viewBox="0 0 24 24" fill="none"><path d="M6 16v-5"/></symbol></svg>"#,
        )?;
        let json = r##"{ "light": {"background": "#fff"}, "dark": {"background": "#000"},
                         "icons": "icons.svg" }"##;
        let theme_path = package.join("theme.json");
        let mut theme = parse_theme(json, &theme_path, "sea", None)?;
        attach_icons(&mut theme, &package, &theme_path).await?;
        let shell = theme.shell.ok_or("no shell")?;
        assert!(shell.get("icons_file").is_none());
        assert_eq!(shell["icons"]["bell"]["body"], r#"<path d="M6 16v-5"/>"#);

        // A sprite with anything but plain shapes refuses the whole plugin.
        std::fs::write(
            package.join("icons.svg"),
            r#"<svg><symbol id="bell" viewBox="0 0 24 24"><script>alert(1)</script></symbol></svg>"#,
        )?;
        let mut bad = parse_theme(json, &theme_path, "sea", None)?;
        let refused = attach_icons(&mut bad, &package, &theme_path).await;
        assert!(matches!(refused, Err(CatalogError::ThemeIcons { .. })));

        // The path must be an .svg inside the plugin.
        let outside = r##"{ "light": {}, "dark": {}, "icons": "../icons.svg" }"##;
        assert!(parse_theme(outside, &theme_path, "sea", None).is_err());
        Ok(())
    }

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
    fn upgrade_all_must_stand_alone() -> Result<(), CatalogError> {
        assert!(!upgrade_all_requested(&["notes".parse()?])?);
        assert!(upgrade_all_requested(&["all".parse()?])?);
        assert!(matches!(
            upgrade_all_requested(&["all@0.1.0".parse()?]),
            Err(CatalogError::InvalidSpec(_))
        ));
        assert!(matches!(
            upgrade_all_requested(&["all".parse()?, "notes".parse()?]),
            Err(CatalogError::InvalidSpec(_))
        ));
        Ok(())
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
        assert!(matches!(
            result,
            Err(CatalogError::FileOutsidePackage { .. })
        ));
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
        assert_eq!(
            (app.route.as_str(), app.label.as_str()),
            ("/chat", "Team Chat")
        );
        assert_eq!(app.icon.as_deref(), Some("message-square"));

        let elsewhere = manifest_with_app("[app]\nroute = \"/other\"\n")?;
        assert!(matches!(
            read_app(&elsewhere, Path::new("plugin.toml"), &pages),
            Err(CatalogError::AppRouteNotAPage { .. })
        ));
        Ok(())
    }

    #[test]
    fn an_app_route_is_a_fixed_page_and_the_icon_a_plain_name()
    -> Result<(), Box<dyn std::error::Error>> {
        let pages = [page("/chat/{channel}")?];
        for app in [
            "[app]\nroute = \"/chat/{channel}\"\n",
            "[app]\nroute = \"chat\"\n",
            "[app]\nroute = \"/chat\"\nicon = \"Not An Icon\"\n",
        ] {
            let manifest = manifest_with_app(app)?;
            assert!(
                read_app(
                    &manifest,
                    Path::new("plugin.toml"),
                    &[page("/chat")?, pages[0].clone()]
                )
                .is_err(),
                "{app}"
            );
        }
        // No `[app]`: not an app.
        let plain = manifest_with_app("")?;
        assert!(read_app(&plain, Path::new("plugin.toml"), &pages)?.is_none());
        Ok(())
    }

    #[test]
    fn the_app_label_can_be_overridden_and_defaults_to_the_plugin()
    -> Result<(), Box<dyn std::error::Error>> {
        let pages = [page("/chat")?];
        let named = manifest_with_app("[app]\nlabel = \"Chat\"\nroute = \"/chat\"\n")?;
        assert_eq!(
            read_app(&named, Path::new("plugin.toml"), &pages)?.map(|app| app.label),
            Some("Chat".to_string())
        );
        Ok(())
    }
}
