use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
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

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            user: "root".to_string(),
            password: "root".to_string(),
            name: "main".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8000,
            pool_size: Some(10),
            ssl_mode: Some(String::new()),
            db_filter: Some(String::new()),
        }
    }
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
    pub plugin_workspace_paths: Option<String>,
    pub storages: Option<Vec<StorageConfig>>,
}

impl Default for AetherConfig {
    fn default() -> Self {
        let mut storages: Vec<StorageConfig> = Vec::new();
        storages.push(StorageConfig {
            storage_type: StorageType::Local,
        });
        Self {
            configuration: Some(CoreConfig::default()),
            database: Some(DatabaseConfig::default()),
            server: Some(ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 7890,
            }),
            plugin_workspace_paths: Some("".to_string()),
            storages: Some(storages),
        }
    }
}

impl AetherConfig {
    pub fn display(&self) {
        if let Some(server) = &self.server {
            log::info!("Server config: {}:{}", server.host, server.port);
        }

        if let Some(storage) = &self.storages {
            log::info!("Storages: {:?}", storage)
        }
    }

    pub fn display_server_start(&self) {
        if let Some(server) = &self.server {
            log::info!(
                "Starting server at: https://{}:{}",
                server.host,
                server.port
            );
        }
    }
}
