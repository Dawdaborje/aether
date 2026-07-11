use crate::config_manager::errors::ConfigError;
use crate::config_manager::models::{AetherConfig, CoreConfig, DatabaseConfig};
use std::fs;
use toml::Value;

fn get_field<'a>(value: &'a Value, key: &str) -> Result<&'a Value, ConfigError> {
    value
        .get(key)
        .ok_or_else(|| ConfigError::MissingField(key.to_string()))
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
        name: require_str(database_value, "namespace")?,
        port: require_port(database_value, "port")?,

        db_filter: Some(String::new()),
        pool_size: Some(10),
        ssl_mode: Some(String::new()),
    })
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
        is_development_mode: require_bool(&value, "is_development_mode")?,
        is_development_with_assets: require_bool(&value, "is_development_with_assets")?,
    };

    Ok(AetherConfig {
        database: Some(db_conf),
        configuration: Some(core_conf),
        ..Default::default()
    })
}

pub fn generate_aether_config(config_file: Option<String>) -> Result<AetherConfig, ConfigError> {
    match &config_file {
        Some(value) => gen_aether_conf_from_config_file(value),
        None => Ok(AetherConfig::default()),
    }
}
