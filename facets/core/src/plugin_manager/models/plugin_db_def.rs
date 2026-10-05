use serde::{Deserialize, Serialize};
use surrealdb::types::{Datetime, RecordId, SurrealValue};

use super::plugin_def::{PluginAuthor, PluginCategory, PluginDefinition};

#[derive(Debug, Clone, Deserialize, Serialize, SurrealValue)]
pub struct PluginDbAuthor {
    pub name: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub github: Option<String>,
    #[serde(default)]
    pub website: Option<String>,
}

impl From<PluginDbAuthor> for PluginAuthor {
    fn from(author: PluginDbAuthor) -> Self {
        Self {
            name: author.name,
            email: author.email,
            github: author.github,
            website: author.website,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, SurrealValue)]
pub struct PluginDbCategory {
    pub name: String,
    #[serde(default)]
    pub label: String,
}

impl From<PluginDbCategory> for PluginCategory {
    fn from(category: PluginDbCategory) -> Self {
        Self {
            name: category.name,
            label: category.label,
        }
    }
}

/// Representation of the `plugins` table in the core database.
#[derive(Debug, Clone, Deserialize, Serialize, SurrealValue)]
pub struct PluginDbDefinition {
    pub id: RecordId,
    pub name: String,
    pub label: String,
    pub version: String,
    pub description: Option<String>,
    pub long_description: Option<String>,
    pub icon_path: Option<String>,
    pub website: Option<String>,
    pub authors: Option<Vec<PluginDbAuthor>>,
    pub categories: Option<Vec<PluginDbCategory>>,
    pub dependencies: Option<Vec<String>>,
    /// The plugin's `[[schedule]]` entries, copied into an organization when it installs it.
    pub schedules: Option<Vec<serde_json::Value>>,
    /// The plugin's `[[command]]` entries, run with `aether --command`.
    pub commands: Option<Vec<serde_json::Value>>,
    /// The plugin's text: `{ "default": "en", "locales": { "en": { key: text } } }`.
    pub i18n: Option<serde_json::Value>,
    /// The plugin's `[[watch]]` entries, copied into an organization when it installs the plugin.
    pub watches: Option<Vec<serde_json::Value>>,
    /// The events the plugin listens to, copied into an organization when it installs the plugin.
    pub event_listeners: Option<Vec<serde_json::Value>>,
    /// The roles the plugin offers, created in an organization when it installs the plugin.
    #[serde(default)]
    pub roles: Option<Vec<serde_json::Value>>,
    pub workspace: Option<String>,
    pub kind: Option<String>,
    pub artifact_path: Option<String>,
    pub artifact_hash: Option<String>,
    /// The folder under `plugins/<name>/` holding this version's file index; none for a
    /// plugin stored in the older layout.
    #[serde(default)]
    pub revision: Option<String>,
    pub is_builtin: bool,
    pub is_active: bool,
    pub date_created: Datetime,
    pub date_updated: Datetime,
}

impl From<PluginDbDefinition> for PluginDefinition {
    fn from(plugin: PluginDbDefinition) -> Self {
        Self {
            name: plugin.name,
            label: plugin.label,
            version: plugin.version,
            description: plugin.description,
            long_description: plugin.long_description,
            icon_path: plugin.icon_path,
            authors: plugin
                .authors
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
            website: plugin.website,
            categories: plugin
                .categories
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
            dependencies: plugin.dependencies.unwrap_or_default(),
            capabilities: Vec::new(),
            access_models: Vec::new(),
            public_functions: Vec::new(),
            public_capabilities: Vec::new(),
            public_access_models: Vec::new(),
            http_hosts: Vec::new(),
            bridges: Vec::new(),
            i18n: None,
            workspace: plugin.workspace,
            kind: plugin.kind,
            is_builtin: plugin.is_builtin,
            api: None,
            meta: None,
            wasm_file: plugin.artifact_path,
            script: None,
            plugin_base_path: String::new(),
        }
    }
}
