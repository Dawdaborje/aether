use crate::config_manager::errors::ConfigError;
use crate::config_manager::models::{
    AetherConfig, CoreConfig, DatabaseConfig, OrgResolutionMode, ServerConfig, TenancyConfig,
};
use aether_cache::{CacheBackendKind, CacheConfig, RedisCacheConfig};
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

fn require_bool(value: &Value, key: &str) -> Result<bool, ConfigError> {
    get_field(value, key)?
        .as_bool()
        .ok_or_else(|| ConfigError::InvalidType {
            field: key.to_string(),
            expected: "bool".to_string(),
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

fn build_database_conf(database_value: &Value) -> Result<DatabaseConfig, ConfigError> {
    if !database_value.is_table() {
        return Err(ConfigError::InvalidType {
            field: "database".to_string(),
            expected: "table".to_string(),
        });
    }

    Ok(DatabaseConfig {
        host: require_str(database_value, "host")?,
        user: require_str(database_value, "user")?,
        password: require_str(database_value, "password")?,
        namespace: require_str(database_value, "namespace")?,
        port: require_port(database_value, "port")?,

        db_filter: Some(String::new()),
        pool_size: Some(10),
        ssl_mode: Some(String::new()),
    })
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

fn bool_from_root_or_table(
    root: &Value,
    table_key: &str,
    field: &str,
) -> Result<bool, ConfigError> {
    if let Some(v) = root.get(field).and_then(|v| v.as_bool()) {
        return Ok(v);
    }
    if let Some(table) = optional_table(root, table_key)? {
        return require_bool(table, field);
    }
    Err(ConfigError::MissingField(format!("{table_key}.{field}")))
}

fn build_server_conf(server_value: &Value) -> Result<ServerConfig, ConfigError> {
    let mut config = ServerConfig::default();
    if let Some(host) = server_value.get("host").and_then(|v| v.as_str()) {
        config.host = host.to_string();
    }
    if server_value.get("port").is_some() {
        config.port = require_port(server_value, "port")?;
    }
    if let Some(paths) = server_value.get("addon_paths").and_then(|v| v.as_array()) {
        config.addon_paths = paths
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
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

pub fn gen_aether_conf_from_config_file(conf_file: &str) -> Result<AetherConfig, ConfigError> {
    log::info!("Using this file for configuration: {}", conf_file);

    let content = fs::read_to_string(conf_file).map_err(|source| ConfigError::Read {
        path: conf_file.to_string(),
        source,
    })?;

    let value: Value = toml::from_str(&content)?;

    let db_conf = build_database_conf(get_field(&value, "database")?)?;

    let core_conf = CoreConfig {
        is_development_mode: bool_from_root_or_table(
            &value,
            "configuration",
            "is_development_mode",
        )?,
        is_development_with_assets: bool_from_root_or_table(
            &value,
            "configuration",
            "is_development_with_assets",
        )?,
    };

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

    Ok(AetherConfig {
        database: Some(db_conf),
        configuration: Some(core_conf),
        server,
        cache,
        tenancy,
        ..Default::default()
    })
}

pub fn generate_aether_config(config_file: Option<String>) -> Result<AetherConfig, ConfigError> {
    match &config_file {
        Some(value) => gen_aether_conf_from_config_file(value),
        None => Ok(AetherConfig::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_toml(body: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().expect("tempfile");
        write!(file, "{body}").expect("write");
        file
    }

    const BASE: &str = r#"
[configuration]
is_development_mode = false
is_development_with_assets = false

[database]
host = "localhost"
port = 8000
user = "root"
password = "root"
namespace = "aether"
"#;

    #[test]
    fn loads_database_namespace_from_toml() {
        let body = format!(
            r#"{BASE}
[cache]
backend = "moka"
"#
        );
        let file = write_toml(&body);

        let conf = gen_aether_conf_from_config_file(file.path().to_str().unwrap()).unwrap();
        let database = conf.database.expect("database");
        assert_eq!(database.namespace, "aether");
    }

    #[test]
    fn loads_moka_cache_section() {
        let body = format!(
            r#"{BASE}
[cache]
backend = "moka"
default_ttl_secs = 120
max_entries = 500
"#
        );
        let file = write_toml(&body);

        let conf = gen_aether_conf_from_config_file(file.path().to_str().unwrap()).unwrap();
        let cache = conf.cache.expect("cache");
        assert_eq!(cache.backend, CacheBackendKind::Moka);
        assert_eq!(cache.default_ttl_secs, Some(120));
        assert_eq!(cache.max_entries, 500);
    }

    #[test]
    fn loads_redis_cache_section_with_url() {
        let body = format!(
            r#"{BASE}
[cache]
backend = "redis"
default_ttl_secs = 60

[cache.redis]
url = "redis://127.0.0.1:6379/1"
key_prefix = "aether:test:"
"#
        );
        let file = write_toml(&body);

        let conf = gen_aether_conf_from_config_file(file.path().to_str().unwrap()).unwrap();
        let cache = conf.cache.expect("cache");
        assert_eq!(cache.backend, CacheBackendKind::Redis);
        let redis = cache.redis.expect("redis");
        assert_eq!(redis.url, "redis://127.0.0.1:6379/1");
        assert_eq!(redis.key_prefix, "aether:test:");
    }

    #[test]
    fn memory_alias_maps_to_moka() {
        let body = format!(
            r#"{BASE}
[cache]
backend = "memory"
"#
        );
        let file = write_toml(&body);

        let conf = gen_aether_conf_from_config_file(file.path().to_str().unwrap()).unwrap();
        assert_eq!(conf.cache.unwrap().backend, CacheBackendKind::Moka);
    }

    #[test]
    fn redis_backend_without_section_errors() {
        let body = format!(
            r#"{BASE}
[cache]
backend = "redis"
"#
        );
        let file = write_toml(&body);

        let err = gen_aether_conf_from_config_file(file.path().to_str().unwrap()).unwrap_err();
        assert!(matches!(err, ConfigError::MissingField(ref f) if f == "cache.redis"));
    }

    #[test]
    fn build_cache_from_loaded_moka_config() {
        let body = format!(
            r#"{BASE}
[cache]
backend = "moka"
"#
        );
        let file = write_toml(&body);
        let conf = gen_aether_conf_from_config_file(file.path().to_str().unwrap()).unwrap();
        let cache = conf.build_cache().expect("build moka cache");
        assert_eq!(cache.backend(), CacheBackendKind::Moka);
        cache.set("ns", "k", "v", None).unwrap();
        assert_eq!(cache.get("ns", "k").unwrap().unwrap().as_slice(), b"v");
    }
}
