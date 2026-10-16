//! The functions plugins offer to the AI assistant (`[[ai_tool]]`), for one organization.

use aether_ai::{ToolSpec, tool_name};
use serde::Deserialize;
use serde_json::Value;
use surrealdb::types::SurrealValue;

use crate::plugin_manager::models::plugin_def::PluginAiToolDef;
use crate::state::AppState;

/// One tool as the assistant sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Tool {
    /// What the model calls it: `<plugin>__<tool>`.
    pub model_name: String,
    pub plugin: String,
    pub def: PluginAiToolDef,
}

impl Tool {
    pub fn new(plugin: &str, def: PluginAiToolDef) -> Self {
        Self { model_name: tool_name(plugin, &def.name), plugin: plugin.to_string(), def }
    }

    pub fn spec(&self) -> ToolSpec {
        let note = if self.def.writes { " (changes data: the person must confirm it)" } else { "" };
        ToolSpec { name: self.model_name.clone(), description: format!("{}{note}", self.def.description.trim()), schema: self.def.schema.clone() }
    }
}

#[derive(Deserialize, SurrealValue)]
struct Installed {
    plugin_name: String,
    version: String,
}

#[derive(Deserialize, SurrealValue)]
struct Catalog {
    name: String,
    version: String,
    ai_tools: Option<Vec<Value>>,
}

/// The tools of every enabled plugin installed in `org_db`, in plugin order.
pub async fn list(state: &AppState, org_db: &str) -> Result<Vec<Tool>, surrealdb::Error> {
    let org = state.org(org_db).await?;
    let mut response = org
        .query("SELECT plugin_name, version FROM installed_plugins WHERE is_enabled = true ORDER BY plugin_name;")
        .await?
        .check()?;
    let installed: Vec<Installed> = response.take(0)?;
    if installed.is_empty() {
        return Ok(Vec::new());
    }
    let names: Vec<String> = installed.iter().map(|row| row.plugin_name.clone()).collect();
    let core = state.core().await?;
    let mut response = core
        .query("SELECT name, version, ai_tools FROM plugins WHERE name IN $names;")
        .bind(("names", names))
        .await?
        .check()?;
    let catalog: Vec<Catalog> = response.take(0)?;
    let mut tools = Vec::new();
    for row in installed {
        let Some(version) = catalog.iter().find(|c| c.name == row.plugin_name && c.version == row.version) else { continue };
        for value in version.ai_tools.clone().unwrap_or_default() {
            if let Ok(def) = serde_json::from_value::<PluginAiToolDef>(value) {
                tools.push(Tool::new(&row.plugin_name, def));
            }
        }
    }
    Ok(tools)
}
