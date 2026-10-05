use crate::cache::CacheConfig;
use local_storage::LocalStorageConfig;
use s3_storage::S3StorageConfig;
use crate::plugin_manager::models::plugin_def::PluginDefinition;
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
    /// Seconds a request may take before the server gives up and answers
    /// `504`, so a stalled database cannot leave the browser loading forever.
    /// Event streams (SSE) are unaffected: only the time to the
    /// response head counts. `0` turns the limit off.
    #[serde(default = "default_request_timeout_secs")]
    pub request_timeout_secs: u64,
}

fn default_request_timeout_secs() -> u64 {
    30
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
            request_timeout_secs: default_request_timeout_secs(),
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

/// `[notifications]` in `aether.toml`: the live event stream and stored notifications.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct NotificationsConfig {
    /// Open event streams one person (or one anonymous address) may hold at once;
    /// each browser tab holds one.
    #[serde(default = "default_max_streams")]
    pub max_streams_per_actor: u32,
    /// Seconds between keep-alive comments on an idle stream. Keeps proxies from
    /// closing it.
    #[serde(default = "default_keepalive_secs")]
    pub keepalive_secs: u64,
    /// Seconds a stream lives before the server closes it and the browser
    /// reconnects. The reconnect checks the session again, so a logged-out
    /// browser stops receiving.
    #[serde(default = "default_stream_lifetime_secs")]
    pub stream_lifetime_secs: u64,
    /// Most notifications replayed to a browser that reconnects.
    #[serde(default = "default_replay_limit")]
    pub replay_limit: u32,
    /// Delete notifications older than this many days; unset keeps them until
    /// they expire.
    pub retention_days: Option<u32>,
}

fn default_max_streams() -> u32 {
    8
}

fn default_keepalive_secs() -> u64 {
    25
}

fn default_stream_lifetime_secs() -> u64 {
    900
}

fn default_replay_limit() -> u32 {
    100
}

impl Default for NotificationsConfig {
    fn default() -> Self {
        Self {
            max_streams_per_actor: default_max_streams(),
            keepalive_secs: default_keepalive_secs(),
            stream_lifetime_secs: default_stream_lifetime_secs(),
            replay_limit: default_replay_limit(),
            retention_days: None,
        }
    }
}

impl NotificationsConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.max_streams_per_actor == 0 {
            return Err("notifications.max_streams_per_actor must be at least 1".into());
        }
        if self.keepalive_secs == 0 || self.stream_lifetime_secs == 0 {
            return Err(
                "notifications.keepalive_secs and stream_lifetime_secs must be at least 1".into(),
            );
        }
        if self.replay_limit == 0 {
            return Err("notifications.replay_limit must be at least 1".into());
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

/// `[plugin_runtime]` in `aether.toml`: how many compiled plugins are kept in memory
/// and how they are compiled. Every size is in megabytes.
///
/// Plugins compile on first use, never at start-up. Compiled modules are kept in a
/// memory cache bounded by `max_compiled_memory_mb`, least recently used first out.
/// `compile_cache` adds an on-disk cache of compiled code so a restart, or a plugin
/// evicted from memory, loads without compiling again.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginRuntimeConfig {
    /// Memory budget for compiled plugins kept ready. A plugin whose estimated size
    /// alone exceeds it is refused.
    #[serde(default = "default_max_compiled_memory_mb")]
    pub max_compiled_memory_mb: u64,
    /// Largest `.wasm` accepted.
    #[serde(default = "default_max_wasm_size_mb")]
    pub max_wasm_size_mb: u64,
    /// How many times larger a compiled module is in memory than its `.wasm`. The
    /// runtime cannot measure this, so a plugin's size is estimated as
    /// `wasm size × factor + engine_overhead_mb`.
    #[serde(default = "default_compiled_size_factor")]
    pub compiled_size_factor: f64,
    /// Fixed memory every compiled plugin costs on top (its own wasmtime engine).
    #[serde(default = "default_engine_overhead_mb")]
    pub engine_overhead_mb: f64,
    /// Compilations that may run at once. Each can use several times the plugin's size
    /// while it runs.
    #[serde(default = "default_max_concurrent_compiles")]
    pub max_concurrent_compiles: u32,
    /// Compilations that may wait for a turn; beyond this callers are told to retry.
    #[serde(default = "default_compile_queue_limit")]
    pub compile_queue_limit: u32,
    /// Seconds a caller waits for a plugin to become ready.
    #[serde(default = "default_compile_timeout_secs")]
    pub compile_timeout_secs: u64,
    /// Memory limit of one running call.
    #[serde(default = "default_instance_memory_mb")]
    pub instance_memory_mb: u32,
    /// Plugin calls that may run at once.
    #[serde(default = "default_max_concurrent_calls")]
    pub max_concurrent_calls: u32,
    #[serde(default)]
    pub compile_cache: CompileCacheConfig,
}

/// `[plugin_runtime.compile_cache]`: compiled code kept on disk (wasmtime's compilation
/// cache). On by default, under `app_dir`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompileCacheConfig {
    #[serde(default = "default_compile_cache_enabled")]
    pub enabled: bool,
    /// Where compiled code is kept; relative paths are inside `app_dir`.
    #[serde(default = "default_compile_cache_directory")]
    pub directory: PathBuf,
    /// Disk budget; the oldest entries are removed past it.
    #[serde(default = "default_compile_cache_max_size_mb")]
    pub max_size_mb: u64,
}

fn default_max_compiled_memory_mb() -> u64 {
    256
}
fn default_max_wasm_size_mb() -> u64 {
    24
}
/// Measured at 9 to 12 times the `.wasm` on dense synthetic code (run the `calibrate`
/// test in `plugin_manager/runtime.rs` with a real plugin to refine it); real code is
/// usually less dense, so 8 leans safe without wasting the budget.
fn default_compiled_size_factor() -> f64 {
    8.0
}
fn default_engine_overhead_mb() -> f64 {
    2.0
}
fn default_max_concurrent_compiles() -> u32 {
    1
}
fn default_compile_queue_limit() -> u32 {
    32
}
fn default_compile_timeout_secs() -> u64 {
    60
}
fn default_instance_memory_mb() -> u32 {
    16
}
fn default_max_concurrent_calls() -> u32 {
    64
}
fn default_compile_cache_enabled() -> bool {
    true
}
fn default_compile_cache_directory() -> PathBuf {
    PathBuf::from("cache/compiled")
}
fn default_compile_cache_max_size_mb() -> u64 {
    1024
}

impl Default for CompileCacheConfig {
    fn default() -> Self {
        Self {
            enabled: default_compile_cache_enabled(),
            directory: default_compile_cache_directory(),
            max_size_mb: default_compile_cache_max_size_mb(),
        }
    }
}

impl Default for PluginRuntimeConfig {
    fn default() -> Self {
        Self {
            max_compiled_memory_mb: default_max_compiled_memory_mb(),
            max_wasm_size_mb: default_max_wasm_size_mb(),
            compiled_size_factor: default_compiled_size_factor(),
            engine_overhead_mb: default_engine_overhead_mb(),
            max_concurrent_compiles: default_max_concurrent_compiles(),
            compile_queue_limit: default_compile_queue_limit(),
            compile_timeout_secs: default_compile_timeout_secs(),
            instance_memory_mb: default_instance_memory_mb(),
            max_concurrent_calls: default_max_concurrent_calls(),
            compile_cache: CompileCacheConfig::default(),
        }
    }
}

impl PluginRuntimeConfig {
    /// Smallest memory budget: below this nothing useful fits.
    pub const MIN_BUDGET_MB: u64 = 16;

    /// The estimated memory of a compiled plugin whose `.wasm` has `wasm_bytes` bytes.
    pub fn estimated_compiled_mb(&self, wasm_bytes: u64) -> f64 {
        wasm_bytes as f64 / (1024.0 * 1024.0) * self.compiled_size_factor + self.engine_overhead_mb
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.max_compiled_memory_mb < Self::MIN_BUDGET_MB {
            return Err(format!(
                "plugin_runtime.max_compiled_memory_mb must be at least {}",
                Self::MIN_BUDGET_MB
            ));
        }
        if self.max_wasm_size_mb == 0 {
            return Err("plugin_runtime.max_wasm_size_mb must be at least 1".into());
        }
        if !self.compiled_size_factor.is_finite() || self.compiled_size_factor < 1.0 {
            return Err("plugin_runtime.compiled_size_factor must be at least 1".into());
        }
        if !self.engine_overhead_mb.is_finite() || self.engine_overhead_mb < 0.0 {
            return Err("plugin_runtime.engine_overhead_mb cannot be negative".into());
        }
        let largest = self.estimated_compiled_mb(self.max_wasm_size_mb * 1024 * 1024);
        if largest > self.max_compiled_memory_mb as f64 {
            return Err(format!(
                "a plugin of max_wasm_size_mb ({}) would be estimated at {largest:.0} MB compiled, more than max_compiled_memory_mb ({}); raise the budget or lower max_wasm_size_mb",
                self.max_wasm_size_mb, self.max_compiled_memory_mb
            ));
        }
        if self.max_concurrent_compiles == 0
            || self.compile_queue_limit == 0
            || self.compile_timeout_secs == 0
            || self.max_concurrent_calls == 0
        {
            return Err(
                "plugin_runtime.max_concurrent_compiles, compile_queue_limit, compile_timeout_secs and max_concurrent_calls must be at least 1"
                    .into(),
            );
        }
        if !(1..=4096).contains(&self.instance_memory_mb) {
            return Err("plugin_runtime.instance_memory_mb must be between 1 and 4096".into());
        }
        if self.compile_cache.enabled && self.compile_cache.max_size_mb < Self::MIN_BUDGET_MB {
            return Err(format!(
                "plugin_runtime.compile_cache.max_size_mb must be at least {}",
                Self::MIN_BUDGET_MB
            ));
        }
        Ok(())
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
    #[serde(default)]
    pub notifications: NotificationsConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub scheduler: SchedulerConfig,
    #[serde(default)]
    pub i18n: I18nConfig,
}

/// `[i18n]` in `aether.toml`.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct I18nConfig {
    /// The language used when neither the person, the organization nor the browser names one
    /// the plugin has text for.
    #[serde(default = "default_locale")]
    pub default_locale: String,
}

fn default_locale() -> String {
    "en".to_string()
}

impl Default for I18nConfig {
    fn default() -> Self {
        Self { default_locale: default_locale() }
    }
}

/// `[security]` in `aether.toml`.
#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct SecurityConfig {
    /// Key that encrypts secret settings (API keys, passwords) in the database. Leave it out
    /// to use the `AETHER_SECRET_KEY` environment variable or an automatically created
    /// `<app_dir>/conf/secret.key`. Every Aether process that reads credentials needs the same key.
    #[serde(default)]
    pub secret_key: Option<String>,
}

/// `[scheduler]` in `aether.toml`: how this process runs background jobs. What the jobs do and
/// how they retry is decided by settings in the database.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SchedulerConfig {
    /// Run the scheduler inside `aether --serve`. A standalone `aether --start-scheduler`
    /// takes over while it is alive.
    #[serde(default = "default_true")]
    pub embedded: bool,
    /// Jobs this process runs at the same time.
    #[serde(default = "default_scheduler_concurrency")]
    pub concurrency: usize,
    /// Seconds between looks for due jobs and tasks. A job enqueued while the HTTP server and
    /// the scheduler can reach each other starts at once; this is the fallback.
    #[serde(default = "default_scheduler_poll_secs")]
    pub poll_secs: u64,
    /// Seconds a worker holds a job before another may take it, if it never reports back.
    #[serde(default = "default_scheduler_lease_secs")]
    pub lease_secs: i64,
    /// Queues this process serves; empty means all of them.
    #[serde(default)]
    pub queues: Vec<String>,
    /// Where a standalone scheduler's control API listens.
    #[serde(default = "default_scheduler_bind")]
    pub bind: String,
}

fn default_true() -> bool {
    true
}

fn default_scheduler_concurrency() -> usize {
    8
}

fn default_scheduler_poll_secs() -> u64 {
    5
}

fn default_scheduler_lease_secs() -> i64 {
    120
}

fn default_scheduler_bind() -> String {
    "127.0.0.1:7895".into()
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            embedded: true,
            concurrency: default_scheduler_concurrency(),
            poll_secs: default_scheduler_poll_secs(),
            lease_secs: default_scheduler_lease_secs(),
            queues: Vec::new(),
            bind: default_scheduler_bind(),
        }
    }
}

impl SchedulerConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.concurrency == 0 || self.concurrency > 256 {
            return Err("scheduler.concurrency must be 1 to 256".into());
        }
        if self.poll_secs == 0 {
            return Err("scheduler.poll_secs must be at least 1".into());
        }
        if self.lease_secs < 10 {
            return Err("scheduler.lease_secs must be at least 10".into());
        }
        self.bind.parse::<std::net::SocketAddr>().map_err(|_| "scheduler.bind must be an address such as 127.0.0.1:7895")?;
        Ok(())
    }
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
            notifications: NotificationsConfig::default(),
            security: SecurityConfig::default(),
            scheduler: SchedulerConfig::default(),
            i18n: I18nConfig::default(),
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
