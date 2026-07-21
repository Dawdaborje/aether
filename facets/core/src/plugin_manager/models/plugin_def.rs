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
            workspace: None,
            kind: Some("addon".into()),
            is_builtin: false,
            api: Some(PluginApi {
                version: default_api_version(),
            }),
            meta: None,
        }
    }
}

impl PluginDefinition {
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
pub struct PluginPageDef {
    pub route: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub view: Option<String>,
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

/// Full `plugin.toml` document. Unknown top-level tables are ignored by serde.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginManifest {
    pub plugin: PluginDefinition,
    #[serde(default)]
    pub models: Vec<PluginModelRef>,
    #[serde(default)]
    pub pages: Vec<PluginPageDef>,
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
}

impl PluginManifest {
    pub fn normalize(&mut self) {
        self.plugin.hoist_meta();
    }
}
