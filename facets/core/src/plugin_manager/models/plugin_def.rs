use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct PluginAuthor {
    pub name: String,
    pub email: String,
    pub github: Option<String>,
    pub website: Option<String>
}

#[derive(Deserialize, Serialize)]
pub struct PluginCategory {
    pub name: String,
    pub label: String,
}

#[derive(Serialize, Deserialize)]
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
    pub dependencies: Vec<String>,
    pub capabilities: Vec<String>,
    pub access_models: Vec<String>,
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
        }
    }
}