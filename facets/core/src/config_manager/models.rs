use crate::cache::CacheConfig;
use local_storage::LocalStorageConfig;
use s3_storage::S3StorageConfig;
use crate::plugin_manager::models::plugin_def::PluginDefinition;
use crate::plugin_manager::runtime::DEFAULT_MAX_COMPILED_PLUGINS;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
            user: "".to_string(),
            password: "".to_string(),
            namespace: "".to_string(),
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
    /// Reverse proxies whose `X-Forwarded-For` header is believed. Without an
    /// entry here the TCP peer address is the client address, so audit rows
    /// cannot be forged with a header.
    #[serde(default)]
    pub trusted_proxies: Vec<std::net::IpAddr>,
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
            trusted_proxies: Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Default, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MediaBackendKind {
    #[default]
    Local,
    S3,
}

/// `[media]` in `aether.toml`: where uploaded files live. The backend is a
/// deployment decision made here, never by a plugin.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MediaConfig {
    #[serde(default)]
    pub backend: MediaBackendKind,
    /// `[media.local]`; defaults to `<app_dir>/media`. Overridden by `--media-dir`.
    #[serde(default)]
    pub local: Option<LocalStorageConfig>,
    /// `[media.s3]`; required when `backend = "s3"`.
    #[serde(default)]
    pub s3: Option<S3StorageConfig>,
}

impl MediaConfig {
    /// Local storage under `<app_dir>/media`.
    pub fn default_for(app_dir: &std::path::Path) -> Self {
        Self {
            backend: MediaBackendKind::Local,
            local: Some(LocalStorageConfig {
                base_path: app_dir.join("media"),
            }),
            s3: None,
        }
    }

    /// Point the local backend at `dir` (the `--media-dir` flag).
    pub fn override_local_dir(&mut self, dir: PathBuf) -> Result<(), String> {
        if self.backend != MediaBackendKind::Local {
            return Err(format!(
                "--media-dir only applies to the local media backend, but media.backend is {:?}",
                self.backend
            ));
        }
        self.local = Some(LocalStorageConfig { base_path: dir });
        Ok(())
    }
}

/// How client IP addresses are stored in the audit trail.
#[derive(Debug, Deserialize, Serialize, Default, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IpStorage {
    /// The full address.
    #[default]
    Full,
    /// IPv4 `/24` and IPv6 `/48`: the network, not the host.
    Truncated,
    /// A keyed hash (needs `ip_hash_key`): rows from the same address still
    /// match each other, but the address cannot be read back.
    Hashed,
}

/// `[audit]` in `aether.toml`: what the page-visit and data-access trail keeps.
#[derive(Deserialize, Serialize, Clone, Default)]
pub struct AuditConfig {
    #[serde(default)]
    pub ip: IpStorage,
    /// Secret for `ip = "hashed"`, at least 16 characters.
    #[serde(default, skip_serializing)]
    pub ip_hash_key: Option<String>,
    /// Delete audit rows older than this many days. Unset keeps them forever.
    #[serde(default)]
    pub retention_days: Option<u32>,
}

impl std::fmt::Debug for AuditConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuditConfig")
            .field("ip", &self.ip)
            .field("ip_hash_key", &self.ip_hash_key.as_ref().map(|_| "<redacted>"))
            .field("retention_days", &self.retention_days)
            .finish()
    }
}

const MIN_IP_HASH_KEY_CHARS: usize = 16;

impl AuditConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.ip == IpStorage::Hashed
            && self
                .ip_hash_key
                .as_deref()
                .is_none_or(|key| key.chars().count() < MIN_IP_HASH_KEY_CHARS)
        {
            return Err(format!(
                "audit.ip = \"hashed\" needs audit.ip_hash_key of at least {MIN_IP_HASH_KEY_CHARS} characters"
            ));
        }
        if self.retention_days == Some(0) {
            return Err("audit.retention_days must be at least 1 (omit it to keep rows forever)".into());
        }
        Ok(())
    }
}

/// `[public]` in `aether.toml`: limits on anonymous traffic.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PublicConfig {
    /// How many requests one client address may make per minute while not
    /// logged in. Logged-in users are not limited. Requests over the limit
    /// get `429` and are not recorded in the audit trail.
    #[serde(default = "default_request_rate")]
    pub max_requests_per_ip_per_minute: u32,
    /// How many new visitor identities one client address may create per
    /// minute. Each visitor is a stored row, so this bounds what bots can create.
    #[serde(default = "default_visitor_rate")]
    pub max_new_visitors_per_ip_per_minute: u32,
}

fn default_visitor_rate() -> u32 {
    30
}

fn default_request_rate() -> u32 {
    300
}

impl Default for PublicConfig {
    fn default() -> Self {
        Self {
            max_requests_per_ip_per_minute: default_request_rate(),
            max_new_visitors_per_ip_per_minute: default_visitor_rate(),
        }
    }
}

impl PublicConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.max_requests_per_ip_per_minute == 0 {
            return Err("public.max_requests_per_ip_per_minute must be at least 1".into());
        }
        Ok(())
    }
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

/// In-process Extism compile cache. Only `CompiledPlugin` objects are retained;
/// a fresh `Plugin` instance is created for every call.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginRuntimeConfig {
    #[serde(default = "default_max_compiled_plugins")]
    pub max_compiled: u64,
}

fn default_max_compiled_plugins() -> u64 {
    DEFAULT_MAX_COMPILED_PLUGINS
}

impl Default for PluginRuntimeConfig {
    fn default() -> Self {
        Self {
            max_compiled: default_max_compiled_plugins(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AetherConfig {
    pub app_dir: PathBuf,
    pub configuration: Option<CoreConfig>,
    pub database: Option<DatabaseConfig>,
    pub server: Option<ServerConfig>,
    pub plugins: Option<Vec<PluginDefinition>>,
    /// Bounded Extism `CompiledPlugin` cache. New catalog versions compile on
    /// first use; LRU eviction keeps RAM from growing with every installed plugin.
    #[serde(default)]
    pub plugin_runtime: PluginRuntimeConfig,
    #[serde(default = "default_media_config")]
    pub media: MediaConfig,
    pub cache: Option<CacheConfig>,
    #[serde(default)]
    pub tenancy: TenancyConfig,
    #[serde(default)]
    pub audit: AuditConfig,
    #[serde(default)]
    pub public: PublicConfig,
}

fn default_media_config() -> MediaConfig {
    MediaConfig::default_for(std::path::Path::new(DEFAULT_APP_DIR))
}

const DEFAULT_APP_DIR: &str = "/opt/aether";

impl Default for AetherConfig {
    fn default() -> Self {
        let app_dir = PathBuf::from(DEFAULT_APP_DIR);
        Self {
            media: MediaConfig::default_for(&app_dir),
            app_dir,
            configuration: Some(CoreConfig::default()),
            database: Some(DatabaseConfig::default()),
            server: Some(ServerConfig::default()),
            plugins: Some(vec![]),
            plugin_runtime: PluginRuntimeConfig::default(),
            cache: Some(CacheConfig::default()),
            tenancy: TenancyConfig::default(),
            audit: AuditConfig::default(),
            public: PublicConfig::default(),
        }
    }
}

impl AetherConfig {
    /// Build the kernel cache from `[cache]` (defaults to in-process Moka).
    pub fn build_cache(&self) -> Result<crate::cache::Cache, crate::cache::CacheError> {
        self.cache.clone().unwrap_or_default().build()
    }

    pub fn display(&self) {
        if let Some(server) = &self.server {
            log::info!("Server config: {}:{}", server.host, server.port);
        }

        log::info!("Media: {:?}", self.media);

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
        log::info!("Audit: {:?}", self.audit);
        log::info!("Application directory: {}", self.app_dir.display());
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
