use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct DatabaseConfig {
    pub user: String,
    pub password: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub pool_size: Option<u32>,
    pub ssl_mode: Option<String>,
    pub db_filter: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Deserialize, Serialize)]
pub struct AetherConfig {
    pub database: DatabaseConfig,
    pub server: ServerConfig,
}

impl AetherConfig {
    pub fn gen_config_file(file_path: &str) {
        println!("{:?}", file_path)
    }
}

pub fn gen_config_file(file_path: &str) {
    println!("{:?}", file_path)
}
