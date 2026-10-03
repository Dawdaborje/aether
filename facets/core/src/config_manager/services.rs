use crate::cache::{CacheBackendKind, CacheConfig, RedisCacheConfig};
use crate::config_manager::errors::{ConfigError, MissingSetting};
use crate::config_manager::models::{
    AetherConfig, AuditConfig, CoreConfig, DatabaseConfig, MediaBackendKind, MediaConfig, OrgResolutionMode,
    PublicConfig, ServerConfig, TenancyConfig,
};
use crate::plugin_manager::models::plugin_def::PluginDefinition;
use std::fs;
use toml::Value;

fn get_field<'a>(value: &'a Value, key: &str) -> Result<&'a Value, ConfigError> {
    value
        .get(key)
        .ok_or_else(|| ConfigError::MissingField(key.to_string()))
}

fn optional_table<'a>(value: &'a Value, key: &str) -> Result<Option<&'a Value>, ConfigError> {
    match value.get(key) {
        None => Ok(None),
        Some(v) if v.is_table() => Ok(Some(v)),
        Some(_) => Err(ConfigError::InvalidType {
            field: key.to_string(),
            expected: "table".to_string(),
        }),
    }
}

fn require_str(value: &Value, key: &str) -> Result<String, ConfigError> {
    get_field(value, key)?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| ConfigError::InvalidType {
            field: key.to_string(),
            expected: "string".to_string(),
        })
}


fn require_port(value: &Value, key: &str) -> Result<u16, ConfigError> {
    let raw = get_field(value, key)?
        .as_integer()
        .ok_or_else(|| ConfigError::InvalidType {
            field: key.to_string(),
            expected: "integer".to_string(),
        })?;

    u16::try_from(raw).map_err(|_| ConfigError::InvalidType {
        field: key.to_string(),
        expected: "integer between 0 and 65535".to_string(),
    })
}

fn optional_u64(value: &Value, key: &str) -> Result<Option<u64>, ConfigError> {
    match value.get(key) {
        None => Ok(None),
        Some(v) => {
            let raw = v.as_integer().ok_or_else(|| ConfigError::InvalidType {
                field: key.to_string(),
                expected: "integer".to_string(),
            })?;
            u64::try_from(raw)
                .map(Some)
                .map_err(|_| ConfigError::InvalidType {
                    field: key.to_string(),
                    expected: "non-negative integer".to_string(),
                })
        }
    }
}

fn optional_usize(value: &Value, key: &str) -> Result<Option<usize>, ConfigError> {
    match value.get(key) {
        None => Ok(None),
        Some(v) => {
            let raw = v.as_integer().ok_or_else(|| ConfigError::InvalidType {
                field: key.to_string(),
                expected: "integer".to_string(),
            })?;
            usize::try_from(raw)
                .map(Some)
                .map_err(|_| ConfigError::InvalidType {
                    field: key.to_string(),
                    expected: "non-negative integer".to_string(),
                })
        }
    }
}


fn parse_cache_backend(raw: &str) -> Result<CacheBackendKind, ConfigError> {
    match raw {
        "moka" | "memory" | "in_memory" | "in-memory" => Ok(CacheBackendKind::Moka),
        "redis" => Ok(CacheBackendKind::Redis),
        other => Err(ConfigError::InvalidType {
            field: "cache.backend".to_string(),
            expected: format!(
                "`moka` (default) or `redis` (got `{other}`; aliases: memory, in_memory)"
            ),
        }),
    }
}

fn build_redis_conf(redis_value: &Value) -> Result<RedisCacheConfig, ConfigError> {
    let mut config = RedisCacheConfig::default();

    if let Some(url) = redis_value.get("url").and_then(|v| v.as_str()) {
        config.url = url.to_string();
    } else if redis_value.get("host").is_some() {
        let host = require_str(redis_value, "host")?;
        let port = redis_value
            .get("port")
            .and_then(|v| v.as_integer())
            .and_then(|p| u16::try_from(p).ok())
            .unwrap_or(6379);
        let db = redis_value
            .get("db")
            .and_then(|v| v.as_integer())
            .unwrap_or(0);
        let password = redis_value
            .get("password")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());

        config.url = match password {
            Some(password) => format!("redis://:{password}@{host}:{port}/{db}"),
            None => format!("redis://{host}:{port}/{db}"),
        };
    }

    if let Some(prefix) = redis_value.get("key_prefix").and_then(|v| v.as_str()) {
        config.key_prefix = prefix.to_string();
    }

    Ok(config)
}

fn build_cache_conf(cache_value: &Value) -> Result<CacheConfig, ConfigError> {
    let mut config = CacheConfig::default();

    if let Some(backend) = cache_value.get("backend").and_then(|v| v.as_str()) {
        config.backend = parse_cache_backend(backend)?;
    }

    if let Some(ttl) = optional_u64(cache_value, "default_ttl_secs")? {
        config.default_ttl_secs = Some(ttl);
    }

    if let Some(max_entries) = optional_usize(cache_value, "max_entries")? {
        config.max_entries = max_entries;
    }

    if let Some(max_value_bytes) = optional_usize(cache_value, "max_value_bytes")? {
        config.max_value_bytes = max_value_bytes;
    }

    if let Some(redis_table) = optional_table(cache_value, "redis")? {
        config.redis = Some(build_redis_conf(redis_table)?);
    }

    if config.backend == CacheBackendKind::Redis && config.redis.is_none() {
        return Err(ConfigError::MissingField("cache.redis".to_string()));
    }

    Ok(config)
}


fn build_server_conf(server_value: &Value) -> Result<ServerConfig, ConfigError> {
    let mut config = ServerConfig::default();
    if let Some(host) = server_value.get("host").and_then(|v| v.as_str()) {
        config.host = host.to_string();
    }
    if server_value.get("port").is_some() {
        config.port = require_port(server_value, "port")?;
    }
    if let Some(proxies) = server_value.get("trusted_proxies") {
        let invalid = || ConfigError::InvalidType {
            field: "server.trusted_proxies".to_string(),
            expected: "array of IP address strings".to_string(),
        };
        config.trusted_proxies = proxies
            .as_array()
            .ok_or_else(invalid)?
            .iter()
            .map(|item| {
                item.as_str()
                    .and_then(|raw| raw.parse().ok())
                    .ok_or_else(invalid)
            })
            .collect::<Result<_, _>>()?;
    }
    Ok(config)
}

fn parse_org_resolution(raw: &str) -> Result<OrgResolutionMode, ConfigError> {
    match raw {
        "session_only" => Ok(OrgResolutionMode::SessionOnly),
        "header" => Ok(OrgResolutionMode::Header),
        "subdomain" => Ok(OrgResolutionMode::Subdomain),
        "path" => Ok(OrgResolutionMode::Path),
        other => Err(ConfigError::InvalidType {
            field: "tenancy.org_resolution".to_string(),
            expected: format!("session_only | header | subdomain | path (got `{other}`)"),
        }),
    }
}

fn build_tenancy_conf(tenancy_value: &Value) -> Result<TenancyConfig, ConfigError> {
    let mut config = TenancyConfig::default();
    if let Some(mode) = tenancy_value.get("org_resolution").and_then(|v| v.as_str()) {
        config.org_resolution = parse_org_resolution(mode)?;
    }
    if let Some(header) = tenancy_value.get("org_header").and_then(|v| v.as_str()) {
        config.org_header = header.to_string();
    }
    if let Some(prefix) = tenancy_value
        .get("org_path_prefix")
        .and_then(|v| v.as_str())
    {
        config.org_path_prefix = prefix.to_string();
    }
    Ok(config)
}

fn build_plugin_paths_conf(
    plugin_paths_value: Option<&Value>,
    config_directory: &std::path::Path,
) -> Result<Vec<String>, ConfigError> {
    let Some(plugin_paths_value) = plugin_paths_value else {
        return Ok(Vec::new());
    };
    if !plugin_paths_value.is_array() {
        return Err(ConfigError::InvalidType {
            field: "plugin_paths".to_string(),
            expected: "array".to_string(),
        });
    }
    let mut paths = Vec::new();
    for item in plugin_paths_value.as_array().unwrap_or(&Vec::new()) {
        if let Some(path) = item.as_str() {
            let path = std::path::Path::new(path);
            let resolved = if path.is_absolute() {
                path.to_path_buf()
            } else {
                config_directory.join(path)
            };
            paths.push(resolved.to_string_lossy().into_owned());
        }
    }
    Ok(paths)
}

/// Relative paths in the config file are relative to the file's directory.
fn resolve_against(path: &std::path::Path, base: &std::path::Path) -> std::path::PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

fn build_media_conf(
    root: &Value,
    app_dir: &std::path::Path,
    config_directory: &std::path::Path,
) -> Result<MediaConfig, ConfigError> {
    let Some(table) = optional_table(root, "media")? else {
        return Ok(MediaConfig::default_for(app_dir));
    };
    let mut media: MediaConfig = table.clone().try_into()?;

    match media.backend {
        MediaBackendKind::Local => {
            let mut local = media.local.take().unwrap_or_else(|| {
                MediaConfig::default_for(app_dir)
                    .local
                    .unwrap_or_default()
            });
            local.base_path = resolve_against(&local.base_path, config_directory);
            media.local = Some(local);
        }
        MediaBackendKind::S3 => {
            if media.s3.is_none() {
                return Err(ConfigError::MissingField("media.s3".to_string()));
            }
        }
    }
    Ok(media)
}

/// Values given on the command line. They override the config file and can
/// supply required settings the file does not contain.
#[derive(Debug, Clone, Default)]
pub struct ConfigOverrides {
    /// Absolute path of the application directory (`--app-dir`).
    pub app_dir: Option<std::path::PathBuf>,
    pub db_host: Option<String>,
    pub db_port: Option<u16>,
    pub db_user: Option<String>,
    pub db_password: Option<String>,
    pub db_namespace: Option<String>,
}

/// A required string: from `flag_value`, else `table[key]`. A missing or empty
/// value is recorded in `missing`; there is no default.
fn text_setting(
    table: Option<&Value>,
    key: &str,
    flag_value: Option<&str>,
    label: &'static str,
    flag: Option<&'static str>,
    missing: &mut Vec<MissingSetting>,
) -> Result<Option<String>, ConfigError> {
    if let Some(value) = flag_value.filter(|value| !value.trim().is_empty()) {
        return Ok(Some(value.to_string()));
    }
    match table.and_then(|table| table.get(key)) {
        None => {
            missing.push(MissingSetting { key: label, flag, empty: false });
            Ok(None)
        }
        Some(Value::String(text)) if text.trim().is_empty() => {
            missing.push(MissingSetting { key: label, flag, empty: true });
            Ok(None)
        }
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err(ConfigError::InvalidType {
            field: label.to_string(),
            expected: "string".to_string(),
        }),
    }
}

/// A required TCP port (1-65535), from `flag_value` or `[database] port`.
fn port_setting(
    table: Option<&Value>,
    flag_value: Option<u16>,
    missing: &mut Vec<MissingSetting>,
) -> Result<Option<u16>, ConfigError> {
    const LABEL: &str = "database.port";
    let invalid = || ConfigError::InvalidType {
        field: LABEL.to_string(),
        expected: "integer between 1 and 65535".to_string(),
    };
    if let Some(port) = flag_value {
        return if port == 0 { Err(invalid()) } else { Ok(Some(port)) };
    }
    match table.and_then(|table| table.get("port")) {
        None => {
            missing.push(MissingSetting { key: LABEL, flag: Some("--db-port"), empty: false });
            Ok(None)
        }
        Some(Value::Integer(raw)) => match u16::try_from(*raw) {
            Ok(port) if port != 0 => Ok(Some(port)),
            _ => Err(invalid()),
        },
        Some(_) => Err(invalid()),
    }
}

/// A required boolean, written at the top level or in `[table_key]`.
fn flag_setting(
    root: &Value,
    table_key: &str,
    field: &str,
    label: &'static str,
    missing: &mut Vec<MissingSetting>,
) -> Result<Option<bool>, ConfigError> {
    let invalid = || ConfigError::InvalidType {
        field: label.to_string(),
        expected: "bool".to_string(),
    };
    let found = match root.get(field) {
        Some(value) => Some(value),
        None => optional_table(root, table_key)?.and_then(|table| table.get(field)),
    };
    match found {
        Some(Value::Boolean(value)) => Ok(Some(*value)),
        Some(_) => Err(invalid()),
        None => {
            missing.push(MissingSetting { key: label, flag: None, empty: false });
            Ok(None)
        }
    }
}

/// Load the configuration from `config_file` (if any) and `overrides`.
///
/// The database connection (`host`, `port`, `user`, `password`, `namespace`),
/// `app_dir` and the two `[configuration]` flags have no defaults: anything not
/// found in the file or given as a flag is reported, all at once, in
/// [`ConfigError::MissingSettings`]. Everything else (server, cache, media,
/// audit, tenancy, ...) has a documented default.
pub fn load_config(
    config_file: Option<&str>,
    overrides: &ConfigOverrides,
) -> Result<AetherConfig, ConfigError> {
    let (value, config_directory) = match config_file {
        Some(path) => {
            log::info!("Using this file for configuration: {path}");
            let content = fs::read_to_string(path).map_err(|source| ConfigError::Read {
                path: path.to_string(),
                source,
            })?;
            let directory = std::path::Path::new(path)
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .to_path_buf();
            (toml::from_str::<Value>(&content)?, directory)
        }
        None => (
            Value::Table(toml::map::Map::new()),
            std::env::current_dir().map_err(|source| ConfigError::Read {
                path: ".".to_string(),
                source,
            })?,
        ),
    };

    let mut missing = Vec::new();
    let database = optional_table(&value, "database")?;
    let host = text_setting(database, "host", overrides.db_host.as_deref(), "database.host", Some("--db-host"), &mut missing)?;
    let port = port_setting(database, overrides.db_port, &mut missing)?;
    let user = text_setting(database, "user", overrides.db_user.as_deref(), "database.user", Some("--db-user"), &mut missing)?;
    let password = text_setting(database, "password", overrides.db_password.as_deref(), "database.password", Some("--db-password"), &mut missing)?;
    let namespace = text_setting(database, "namespace", overrides.db_namespace.as_deref(), "database.namespace", Some("--db-namespace"), &mut missing)?;

    let app_dir = match overrides.app_dir.clone() {
        Some(path) => Some(path),
        None => text_setting(Some(&value), "app_dir", None, "app_dir", Some("--app-dir"), &mut missing)?
            .map(|raw| resolve_against(std::path::Path::new(&raw), &config_directory)),
    };
    let is_development_mode = flag_setting(&value, "configuration", "is_development_mode", "configuration.is_development_mode", &mut missing)?;
    let is_development_with_assets = flag_setting(&value, "configuration", "is_development_with_assets", "configuration.is_development_with_assets", &mut missing)?;

    let (
        Some(host),
        Some(port),
        Some(user),
        Some(password),
        Some(namespace),
        Some(app_dir),
        Some(is_development_mode),
        Some(is_development_with_assets),
    ) = (host, port, user, password, namespace, app_dir, is_development_mode, is_development_with_assets)
    else {
        return Err(ConfigError::MissingSettings {
            file: config_file.map(str::to_string),
            missing,
        });
    };

    let media = build_media_conf(&value, &app_dir, &config_directory)?;
    let audit: AuditConfig = match optional_table(&value, "audit")? {
        Some(table) => table.clone().try_into()?,
        None => AuditConfig::default(),
    };
    audit.validate().map_err(|message| ConfigError::InvalidType {
        field: "audit".to_string(),
        expected: message,
    })?;
    let public: PublicConfig = match optional_table(&value, "public")? {
        Some(table) => table.clone().try_into()?,
        None => PublicConfig::default(),
    };
    public.validate().map_err(|message| ConfigError::InvalidType {
        field: "public".to_string(),
        expected: message,
    })?;

    let server = match optional_table(&value, "server")? {
        Some(table) => Some(build_server_conf(table)?),
        None => Some(ServerConfig::default()),
    };
    let cache = match optional_table(&value, "cache")? {
        Some(table) => Some(build_cache_conf(table)?),
        None => Some(CacheConfig::default()),
    };
    let tenancy = match optional_table(&value, "tenancy")? {
        Some(table) => build_tenancy_conf(table)?,
        None => TenancyConfig::default(),
    };
    let plugins: Vec<PluginDefinition> = Vec::new();

    Ok(AetherConfig {
        app_dir,
        database: Some(DatabaseConfig {
            host,
            port,
            user,
            password,
            namespace,
            pool_size: Some(10),
            ssl_mode: Some(String::new()),
            db_filter: Some(String::new()),
        }),
        configuration: Some(CoreConfig {
            is_development_mode,
            is_development_with_assets,
        }),
        server,
        cache,
        tenancy,
        plugins: Some(plugins),
        media,
        audit,
        public,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn parse(toml_text: &str) -> Result<MediaConfig, ConfigError> {
        let value: Value = toml::from_str(toml_text)?;
        build_media_conf(&value, Path::new("/srv/app"), Path::new("/etc/aether"))
    }

    #[test]
    fn media_defaults_to_local_under_app_dir() -> Result<(), ConfigError> {
        let media = parse("")?;
        assert_eq!(media.backend, MediaBackendKind::Local);
        assert_eq!(
            media.local.map(|l| l.base_path),
            Some(Path::new("/srv/app/media").to_path_buf())
        );
        Ok(())
    }

    #[test]
    fn relative_local_media_dir_is_resolved_against_the_config_directory() -> Result<(), ConfigError>
    {
        let media = parse("[media]\nbackend = \"local\"\n[media.local]\nbase_path = \"files\"\n")?;
        assert_eq!(
            media.local.map(|l| l.base_path),
            Some(Path::new("/etc/aether/files").to_path_buf())
        );
        Ok(())
    }

    #[test]
    fn s3_backend_requires_s3_settings() {
        assert!(matches!(
            parse("[media]\nbackend = \"s3\"\n"),
            Err(ConfigError::MissingField(field)) if field == "media.s3"
        ));
    }

    #[test]
    fn parses_s3_settings() -> Result<(), ConfigError> {
        let media = parse(
            "[media]\nbackend = \"s3\"\n[media.s3]\nbucket = \"b\"\nendpoint = \"http://garage:3900\"\nallow_http = true\n",
        )?;
        let s3 = media.s3.ok_or_else(|| ConfigError::MissingField("media.s3".into()))?;
        assert_eq!(s3.bucket, "b");
        assert!(s3.allow_http);
        Ok(())
    }

    /// Like [`load`], but keeps the typed error.
    fn load_typed(toml_text: &str) -> Result<AetherConfig, ConfigError> {
        let directory = tempfile::tempdir().map_err(|source| ConfigError::Read {
            path: "tempdir".into(),
            source,
        })?;
        let path = directory.path().join("aether.toml");
        std::fs::write(&path, toml_text).map_err(|source| ConfigError::Read {
            path: path.to_string_lossy().into_owned(),
            source,
        })?;
        load_config(Some(&path.to_string_lossy()), &ConfigOverrides::default())
    }

    fn load(toml_text: &str) -> Result<AetherConfig, Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("aether.toml");
        std::fs::write(&path, toml_text)?;
        Ok(load_config(Some(&path.to_string_lossy()), &ConfigOverrides::default())?)
    }

    const BASE: &str = "app_dir = \"app\"\n[database]\nhost=\"h\"\nport=1\nuser=\"u\"\npassword=\"p\"\nnamespace=\"n\"\n[configuration]\nis_development_mode=false\nis_development_with_assets=false\n";

    #[test]
    fn audit_and_public_default_to_keeping_everything() -> Result<(), Box<dyn std::error::Error>> {
        let config = load(BASE)?;
        assert_eq!(config.audit.ip, crate::config_manager::models::IpStorage::Full);
        assert_eq!(config.audit.retention_days, None);
        assert_eq!(config.public.max_new_visitors_per_ip_per_minute, 30);
        assert_eq!(config.public.max_requests_per_ip_per_minute, 300);
        assert!(config.server.map(|s| s.trusted_proxies.is_empty()).unwrap_or(true));
        Ok(())
    }

    #[test]
    fn parses_audit_options_and_trusted_proxies() -> Result<(), Box<dyn std::error::Error>> {
        let config = load(&format!(
            "{BASE}[server]\ntrusted_proxies = [\"10.0.0.1\", \"::1\"]\n[audit]\nip = \"hashed\"\nip_hash_key = \"0123456789abcdef\"\nretention_days = 90\n[public]\nmax_new_visitors_per_ip_per_minute = 5\nmax_requests_per_ip_per_minute = 60\n"
        ))?;
        assert_eq!(config.audit.ip, crate::config_manager::models::IpStorage::Hashed);
        assert_eq!(config.audit.retention_days, Some(90));
        assert_eq!(config.public.max_new_visitors_per_ip_per_minute, 5);
        assert_eq!(config.public.max_requests_per_ip_per_minute, 60);
        assert_eq!(config.server.map(|s| s.trusted_proxies.len()), Some(2));
        assert!(!format!("{:?}", config.audit).contains("0123456789abcdef"));
        Ok(())
    }

    #[test]
    fn rejects_unsafe_audit_settings() {
        assert!(load(&format!("{BASE}[audit]\nip = \"hashed\"\n")).is_err());
        assert!(load(&format!("{BASE}[audit]\nip = \"hashed\"\nip_hash_key = \"short\"\n")).is_err());
        assert!(load(&format!("{BASE}[audit]\nretention_days = 0\n")).is_err());
        assert!(load(&format!("{BASE}[public]\nmax_requests_per_ip_per_minute = 0\n")).is_err());
        assert!(load(&format!("{BASE}[audit]\nip = \"nonsense\"\n")).is_err());
        assert!(load(&format!("{BASE}[server]\ntrusted_proxies = [\"not an ip\"]\n")).is_err());
    }

    fn missing_keys(error: ConfigError) -> Vec<&'static str> {
        match error {
            ConfigError::MissingSettings { missing, .. } => {
                missing.into_iter().map(|setting| setting.key).collect()
            }
            other => panic!("expected MissingSettings, got: {other}"),
        }
    }

    #[test]
    fn nothing_is_assumed_when_there_is_no_config_file() {
        let Err(error) = load_config(None, &ConfigOverrides::default()) else {
            panic!("a configuration with no file and no flags must not load");
        };
        assert_eq!(
            missing_keys(error),
            [
                "database.host",
                "database.port",
                "database.user",
                "database.password",
                "database.namespace",
                "app_dir",
                "configuration.is_development_mode",
                "configuration.is_development_with_assets",
            ]
        );
    }

    #[test]
    fn an_empty_file_reports_everything_missing_instead_of_defaulting() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("aether.toml");
        std::fs::write(&path, "")?;
        let Err(error) = load_config(Some(&path.to_string_lossy()), &ConfigOverrides::default()) else {
            panic!("an empty config must not load");
        };
        let message = error.to_string();
        assert!(message.contains(&path.to_string_lossy().to_string()));
        assert!(message.contains("database.password: is not set. Set `password` under [database] in the config file, or pass --db-password."));
        assert_eq!(missing_keys(error).len(), 8);
        Ok(())
    }

    #[test]
    fn only_the_absent_setting_is_reported() {
        let toml = BASE.replace("password=\"p\"\n", "");
        let Err(error) = load(&toml) else { panic!("password is required") };
        let message = error.to_string();
        assert!(message.contains("database.password"));
        assert!(!message.contains("database.host"));
    }

    #[test]
    fn an_empty_value_counts_as_missing() {
        let toml = BASE.replace("user=\"u\"", "user=\"  \"");
        let Err(error) = load(&toml) else { panic!("an empty user is not a user") };
        assert!(error.to_string().contains("database.user: is empty"));
    }

    #[test]
    fn flags_can_supply_everything_the_file_lacks() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("aether.toml");
        std::fs::write(&path, "[configuration]\nis_development_mode = true\nis_development_with_assets = false\n")?;
        let overrides = ConfigOverrides {
            app_dir: Some(directory.path().join("flag_app")),
            db_host: Some("db.internal".into()),
            db_port: Some(9000),
            db_user: Some("svc".into()),
            db_password: Some("secret".into()),
            db_namespace: Some("prod".into()),
        };
        let config = load_config(Some(&path.to_string_lossy()), &overrides)?;
        let database = config.database.ok_or("no database config")?;
        assert_eq!(
            (database.host.as_str(), database.port, database.user.as_str(), database.namespace.as_str()),
            ("db.internal", 9000, "svc", "prod")
        );
        assert_eq!(config.app_dir, directory.path().join("flag_app"));
        Ok(())
    }

    #[test]
    fn a_flag_beats_the_file() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("aether.toml");
        std::fs::write(&path, BASE)?;
        let config = load_config(
            Some(&path.to_string_lossy()),
            &ConfigOverrides { db_host: Some("flag-host".into()), ..ConfigOverrides::default() },
        )?;
        assert_eq!(config.database.map(|d| d.host), Some("flag-host".to_string()));
        Ok(())
    }

    #[test]
    fn wrong_types_are_type_errors_not_missing_settings() {
        for toml in [
            BASE.replace("port=1", "port=\"1\""),
            BASE.replace("port=1", "port=0"),
            BASE.replace("port=1", "port=70000"),
            BASE.replace("host=\"h\"", "host=5"),
        ] {
            assert!(matches!(load_typed(&toml), Err(ConfigError::InvalidType { .. })), "{toml}");
        }
    }
}
