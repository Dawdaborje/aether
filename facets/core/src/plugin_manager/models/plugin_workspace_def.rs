use serde::{Deserialize, Serialize};

use super::plugin_def::PluginDefinition;

#[derive(Debug, Deserialize, Serialize)]
pub struct WorkspaceAddon {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PluginWorkspaceDefinition {
    pub name: String,
    pub label: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub long_description: Option<String>,
    pub group_label: Option<String>,
    pub group_icon: Option<String>,
    /// Workspace-level soft deps on other workspaces (e.g. erp → crm).
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub dependants: Vec<String>,
    #[serde(default)]
    pub addons: Vec<WorkspaceAddon>,
    /// Resolved plugin manifests (filled after loading each addon path).
    #[serde(default)]
    pub plugins: Vec<PluginDefinition>,
}
