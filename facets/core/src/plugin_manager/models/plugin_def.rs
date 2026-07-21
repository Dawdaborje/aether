use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct PluginAuthor {
    pub name: String,
    pub email: String,
    pub github: Option<String>,
    pub website: Option<String>
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PluginCategory {
    pub name: String,
    pub label: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PluginDefinition {
    pub name: String,
    pub label: String,
    pub version: String,
    pub description: Option<String>,
    pub long_description: Option<String>,
    pub icon_path: Option<String>,
    pub authors: Vec<PluginAuthor>,
    pub website: Option<String>,
    pub categories: Vec<PluginCategory>,
    /// Plugin names this addon depends on (mirrored to plugin_depends_on graph).
    pub dependencies: Vec<String>,
    pub capabilities: Vec<String>,
    pub access_models: Vec<String>,
    /// Parent workspace name (e.g. "base", "erp").
    #[serde(default)]
    pub workspace: Option<String>,
    /// "addon" | "theme" | …
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub is_builtin: bool,
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
        }
    }
}