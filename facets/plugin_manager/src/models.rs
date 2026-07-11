use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Plugin {
    pub path: String,
    pub name: String,
    pub workspace: Option<PluginWorkspace>,
    pub description: Option<String>,
    pub long_description: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
    pub license: Option<String>,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub documentation: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub categories: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PluginWorkspace {
    pub path: String,
    pub name: String,
    pub description: String,
    pub group_name: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub plugins: Vec<Plugin>,
}