use super::plugin_def::PluginDefinition;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct PluginWorkspaceDefinition {
    pub plugins: Vec<PluginDefinition>,
    pub group: String,
}
