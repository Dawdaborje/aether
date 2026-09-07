use aether_cache::CacheConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DatabaseConfig {
    pub user: String,
    pub password: String,
    /// Surreal namespace (from TOML `database.namespace`).
    pub namespace: String,
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
            namespace: "main".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8000,
            pool_size: Some(10),
            ssl_mode: Some(String::new()),
            db_filter: Some(String::new()),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_host() -> String {
    "0.0.0.0".into()
}

fn default_port() -> u16 {
    7890
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub enum StorageType {
    #[default]
    Local,
    S3,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct StorageConfig {
    pub storage_type: StorageType,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct CoreConfig {
    pub is_development_mode: bool,
    pub is_development_with_assets: bool,
}

/// How an HTTP request picks an org for settings override / tenancy.
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrgResolutionMode {
    /// Login / unauthenticated requests use global settings only.
    /// Org override applies once the session has `org_database_id`.
    #[default]
    SessionOnly,
    /// `X-Org-Slug` (or `org_header`) on the request.
    Header,
    /// First subdomain label (`acme.aether.local` → `acme`).
    Subdomain,
    /// Path prefix `/o/{slug}/…` (or `org_path_prefix`).
    Path,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TenancyConfig {
    #[serde(default)]
    pub org_resolution: OrgResolutionMode,
    #[serde(default = "default_org_header")]
    pub org_header: String,
    #[serde(default = "default_org_path_prefix")]
    pub org_path_prefix: String,
}

fn default_org_header() -> String {
    "X-Org-Slug".into()
}

fn default_org_path_prefix() -> String {
    "/o".into()
}

impl Default for TenancyConfig {
    fn default() -> Self {
        Self {
            org_resolution: OrgResolutionMode::SessionOnly,
            org_header: default_org_header(),
            org_path_prefix: default_org_path_prefix(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AetherConfig {
    pub configuration: Option<CoreConfig>,
    pub database: Option<DatabaseConfig>,
    pub server: Option<ServerConfig>,
    pub plugin_paths: Option<Vec<String>>,
    pub storages: Option<Vec<StorageConfig>>,
    pub cache: Option<CacheConfig>,
    #[serde(default)]
    pub tenancy: TenancyConfig,
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
            server: Some(ServerConfig::default()),
            plugin_paths: Some(vec![]),
            storages: Some(storages),
            cache: Some(CacheConfig::default()),
            tenancy: TenancyConfig::default(),
        }
    }
}

impl AetherConfig {
    /// Build the kernel cache from `[cache]` (defaults to in-process Moka).
    pub fn build_cache(&self) -> Result<aether_cache::Cache, aether_cache::CacheError> {
        self.cache.clone().unwrap_or_default().build()
    }

    pub fn display(&self) {
        if let Some(server) = &self.server {
            log::info!("Server config: {}:{}", server.host, server.port);
        }

        if let Some(storage) = &self.storages {
            log::info!("Storages: {:?}", storage)
        }

        if let Some(cache) = &self.cache {
            log::info!(
                "Cache: backend={:?} default_ttl_secs={:?} max_entries={}",
                cache.backend,
                cache.default_ttl_secs,
                cache.max_entries
            );
            if let Some(redis) = &cache.redis {
                log::info!(
                    "Cache redis: url={} key_prefix={}",
                    redis.url,
                    redis.key_prefix
                );
            }
        }

        log::info!("Tenancy org_resolution={:?}", self.tenancy.org_resolution);
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
