use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct DatabaseConfig {
    user: String,
    db_name: String,
    host: String,
}

#[derive(Deserialize, Serialize)]
pub struct AetherConfig {}

impl AetherConfig {
    pub fn gen_config_file(file_path: &str) {}
}
