use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct PluginDefinition {
    pub name: String,
    pub label: String,
    pub version: String,
    pub description: String,
    pub long_description: String,
    pub icon_path: String,
}
