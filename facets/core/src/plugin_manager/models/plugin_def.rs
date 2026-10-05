use serde::{Deserialize, Serialize};

/// Contract version the addon was authored against (`[plugin.api]`).
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginApi {
    #[serde(default = "default_api_version")]
    pub version: String,
}

fn default_api_version() -> String {
    "0.1".into()
}

/// Optional metadata (`[plugin.meta]`). Prefer this over top-level kind/workspace long-term.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginMeta {
    /// `"addon"` | `"theme"` | `"bridge_pack"` | …
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginAuthor {
    pub name: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub github: Option<String>,
    #[serde(default)]
    pub website: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginCategory {
    pub name: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AccessModelDef {
    pub name: String,
    #[serde(default)]
    pub permissions: Vec<String>,
}

/// Core `[plugin]` table. Extra future keys are ignored (serde default).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginDefinition {
    pub name: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub long_description: Option<String>,
    #[serde(default)]
    pub icon_path: Option<String>,
    #[serde(default)]
    pub authors: Vec<PluginAuthor>,
    #[serde(default)]
    pub website: Option<String>,
    #[serde(default)]
    pub categories: Vec<PluginCategory>,
    /// Plugin names this addon depends on (mirrored to `plugin_depends_on`).
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub access_models: Vec<AccessModelDef>,
    /// Functions anonymous visitors may call (`POST /api/plugins/{plugin}/{function}`).
    /// Empty by default: nothing is callable without logging in.
    #[serde(default)]
    pub public_functions: Vec<String>,
    /// Capabilities an anonymous visitor may use, a subset of `capabilities`.
    /// Public pages imply read access (`db::query`) to the models they show;
    /// anything beyond reading, such as `db::mutate`, must be listed here.
    #[serde(default)]
    pub public_capabilities: Vec<String>,
    /// Models anonymous visitors may access beyond what public pages show,
    /// with their permissions. A model listed with `write` is writable by
    /// anyone, so keep this list short.
    #[serde(default)]
    pub public_access_models: Vec<AccessModelDef>,
    /// Outside hosts `http::request` may call, such as `"api.stripe.com"` or `"*.example.com"`
    /// (a wildcard covers subdomains, not the bare domain). Empty by default: no access.
    #[serde(default)]
    pub http_hosts: Vec<String>,
    /// Parent workspace name (e.g. `"base"`, `"erp"`). Prefer `[plugin.meta].workspace`.
    #[serde(default)]
    pub workspace: Option<String>,
    /// `"addon"` | `"theme"` | … Prefer `[plugin.meta].kind`.
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub is_builtin: bool,
    #[serde(default)]
    pub api: Option<PluginApi>,
    #[serde(default)]
    pub meta: Option<PluginMeta>,
    /// The plugin's WebAssembly module (any language that compiles to WASM).
    #[serde(default)]
    pub wasm_file: Option<String>,
    /// The plugin's Rhai script, instead of a WASM module: for small plugins that only read and
    /// write records. A plugin has one or the other.
    #[serde(default)]
    pub script: Option<String>,
    #[serde(default)]
    pub plugin_base_path: String,
}

impl Default for PluginDefinition {
    fn default() -> Self {
        Self {
            name: String::new(),
            label: String::new(),
            version: String::new(),
            description: None,
            long_description: None,
            icon_path: None,
            authors: Vec::new(),
            website: None,
            categories: Vec::new(),
            dependencies: Vec::new(),
            capabilities: Vec::new(),
            access_models: Vec::new(),
            public_functions: Vec::new(),
            public_capabilities: Vec::new(),
            public_access_models: Vec::new(),
            http_hosts: Vec::new(),
            workspace: None,
            kind: Some("addon".into()),
            is_builtin: false,
            api: Some(PluginApi {
                version: default_api_version(),
            }),
            meta: None,
            wasm_file: None,
            script: None,
            plugin_base_path: String::new(),
        }
    }
}

impl PluginDefinition {
    /// The file that holds the plugin's code: its WASM module or its script.
    pub fn code_file(&self) -> Option<&str> {
        self.wasm_file
            .as_deref()
            .or(self.script.as_deref())
            .filter(|file| !file.is_empty())
    }

    /// Whether the code is a Rhai script.
    pub fn is_script(&self) -> bool {
        self.wasm_file.as_deref().is_none_or(str::is_empty) && self.script.as_deref().is_some_and(|s| !s.is_empty())
    }

    /// Copy `[plugin.meta]` into flat `kind` / `workspace` when those are unset.
    pub fn hoist_meta(&mut self) {
        let Some(meta) = self.meta.clone() else {
            return;
        };
        if self.kind.is_none() {
            self.kind = meta.kind;
        }
        if self.workspace.is_none() {
            self.workspace = meta.workspace;
        }
    }

    pub fn api_version(&self) -> &str {
        self.api
            .as_ref()
            .map(|a| a.version.as_str())
            .unwrap_or("0.1")
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginModelRef {
    pub name: String,
    #[serde(default)]
    pub table: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginMenuDef {
    pub name: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub order: Option<i32>,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default = "default_true")]
    pub visible: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginHookDef {
    pub name: String,
    #[serde(default)]
    pub phase: Option<String>,
    #[serde(default)]
    pub event: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub handler: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginEventDef {
    pub name: String,
    /// `"emit"` | `"listen"`
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default)]
    pub payload: Option<String>,
    #[serde(default)]
    pub handler: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginPermissionDef {
    pub key: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginCommunication {
    #[serde(default)]
    pub channels: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginThemeDef {
    pub name: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub is_system: bool,
    #[serde(default)]
    pub tokens_file: Option<String>,
}

/// `[app]`: makes the plugin a tile on the Apps launcher.
///
/// ```toml
/// [app]
/// label = "Chat"              # defaults to the plugin's label
/// icon = "message-square"     # a lucide icon name; the tile shows a letter if unknown
/// route = "/chat"             # a page of this plugin; what the tile opens
/// description = "Team messaging"
/// ```
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginAppDef {
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    pub route: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// Full `plugin.toml` document. Unknown top-level tables are ignored by serde.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginManifest {
    pub plugin: PluginDefinition,
    #[serde(default)]
    pub models: Vec<PluginModelRef>,
    /// Present only to reject the removed `[[pages]]` table with a clear error.
    #[serde(default, rename = "pages")]
    legacy_pages: Option<toml::Value>,
    #[serde(default)]
    pub menus: Vec<PluginMenuDef>,
    #[serde(default)]
    pub hooks: Vec<PluginHookDef>,
    #[serde(default)]
    pub events: Vec<PluginEventDef>,
    #[serde(default)]
    pub permissions: Vec<PluginPermissionDef>,
    #[serde(default)]
    pub communication: PluginCommunication,
    #[serde(default)]
    pub theme: Option<PluginThemeDef>,
    /// Present when the plugin is an app (a tile on the Apps launcher).
    #[serde(default)]
    pub app: Option<PluginAppDef>,
    /// Recurring tasks (`[[schedule]]`): each runs one of the plugin's functions as a
    /// background job on a cron expression or interval in every organization that installs it.
    #[serde(default)]
    pub schedule: Vec<crate::scheduler::TaskDef>,
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("{0}")]
    Toml(#[from] toml::de::Error),

    #[error(
        "`[[pages]]` is no longer supported: declare each page as an XML file under `pages/` \
         with `<page route=\"/…\">` as the root element"
    )]
    LegacyPages,

    #[error(
        "`db::surql` no longer exists: plugins reach the database only through the structured \
         `db::*` commands on the models they are granted; remove it from `{0}`"
    )]
    RawSurql(&'static str),

    #[error("`[[schedule]]`: {0}")]
    Schedule(String),

    #[error(
        "`[[models]]` is no longer supported: define each model in `models/<name>.json` \
         (`aether --sync-models` assigns the ids)"
    )]
    LegacyModels,
}

impl PluginManifest {
    /// Parse and normalise a `plugin.toml`. Unknown keys are ignored so the
    /// contract can grow; the removed `[[pages]]` table is an error.
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        let mut manifest: Self = toml::from_str(text)?;
        if manifest.legacy_pages.is_some() {
            return Err(ManifestError::LegacyPages);
        }
        if !manifest.models.is_empty() {
            return Err(ManifestError::LegacyModels);
        }
        const REMOVED: &str = "db::surql";
        if manifest.plugin.capabilities.iter().any(|c| c == REMOVED) {
            return Err(ManifestError::RawSurql("capabilities"));
        }
        if manifest.plugin.public_capabilities.iter().any(|c| c == REMOVED) {
            return Err(ManifestError::RawSurql("public_capabilities"));
        }
        let mut seen = std::collections::HashSet::new();
        for task in &manifest.schedule {
            task.validate().map_err(|error| ManifestError::Schedule(error.to_string()))?;
            if !seen.insert(task.name.clone()) {
                return Err(ManifestError::Schedule(format!("two tasks are named `{}`", task.name)));
            }
        }
        manifest.normalize();
        Ok(manifest)
    }

    pub fn normalize(&mut self) {
        self.plugin.hoist_meta();
    }
}
