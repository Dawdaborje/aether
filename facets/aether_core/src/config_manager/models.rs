use serde::de::Error as SerdeDeError;
use serde::{Deserialize, Serialize};
use std::fs;
use toml::de::Error;

#[derive(Debug, Deserialize, Serialize, Default)]
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

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub enum StorageType {
    #[default]
    Local,
    S3,
    GCS,
    Azure,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct StorageConfig {
    pub storage_type: StorageType,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct CoreConfig {}

#[derive(Debug, Deserialize, Serialize)]
pub struct AetherConfig {
    pub configuration: Option<CoreConfig>,
    pub database: Option<DatabaseConfig>,
    pub server: Option<ServerConfig>,
}

impl Default for AetherConfig {
    fn default() -> Self {
        Self {
            configuration: Some(CoreConfig::default()),
            database: Some(DatabaseConfig::default()),
            server: Some(ServerConfig::default()),
        }
    }
}

impl AetherConfig {
    pub fn gen_config_from_file(file_path: &str) -> Result<Self, Error> {
        let data =
            fs::read_to_string(file_path).map_err(|e| SerdeDeError::custom(e.to_string()))?;
        toml::from_str(&data)
    }
}

pub fn gen_config_file(file_path: &str) -> Result<AetherConfig, String> {
    AetherConfig::gen_config_from_file(file_path).map_err(|e| e.to_string())
}
