use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::plugin_def::PluginAuthor;

/// One entry of `[workspace.plugins]`: `company = { path = "./company" }`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct WorkspacePluginRef {
    /// Plugin directory, relative to the directory holding `workspace.toml`.
    pub path: String,
}

/// The `[workspace]` table of a plugin workspace's `workspace.toml`.
#[derive(Debug, Deserialize, Serialize)]
pub struct PluginWorkspaceDefinition {
    pub name: String,
    pub label: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub long_description: Option<String>,
    #[serde(default)]
    pub authors: Vec<PluginAuthor>,
    pub group_label: Option<String>,
    pub group_icon: Option<String>,
    /// Workspace-level soft deps on other workspaces (e.g. erp → crm).
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub dependants: Vec<String>,
    /// The only supported way to declare member plugins, keyed by plugin name.
    #[serde(default)]
    pub plugins: BTreeMap<String, WorkspacePluginRef>,
}

/// A whole `workspace.toml`.
#[derive(Debug, Deserialize, Serialize)]
pub struct PluginWorkspaceManifest {
    pub workspace: PluginWorkspaceDefinition,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_plugins_table() -> Result<(), toml::de::Error> {
        let manifest: PluginWorkspaceManifest = toml::from_str(
            r#"
[workspace]
name = "base"

[workspace.plugins]
company = { path = "./company" }
currency = { path = "./currency" }
"#,
        )?;
        assert_eq!(manifest.workspace.plugins.len(), 2);
        assert_eq!(manifest.workspace.plugins["company"].path, "./company");
        Ok(())
    }
}
