use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct RouteDef {
    pub name: String,
    pub path: String,
    pub page_json_file_path: String,
}

#[derive(Deserialize, Serialize)]
pub struct PluginRouteDef {
    pub routes: Vec<RouteDef>,
}
