use crate::config_manager::models::{AetherConfig, DatabaseConfig};
use std::{fs, process::exit};
use toml::Value;

fn build_database_conf(database_value: Value) -> DatabaseConfig {
    if let Value::Table(table) = database_value {
        DatabaseConfig {
            host: table["host"]
                .as_str()
                .expect("host must be string")
                .to_string(),

            user: table["user"]
                .as_str()
                .expect("user must be string")
                .to_string(),

            password: table["password"]
                .as_str()
                .expect("password must be string")
                .to_string(),

            name: table["namespace"]
                .as_str()
                .expect("namespace must be string")
                .to_string(),

            port: table["port"].as_integer().expect("port must be integer") as u16,

            db_filter: Some(String::new()),
            pool_size: Some(10),
            ssl_mode: Some(String::new()),
        }
    } else {
        log::error!("database config should be a table");
        exit(1)
    }
}

pub fn gen_aether_conf_from_config_file(conf_file: &str) -> AetherConfig {
    log::info!("Using this file for configuration: {}", conf_file);

    match fs::read_to_string(conf_file) {
        Ok(content) => {
            let value: Value = toml::from_str(&content).expect("Error parsing the config content");

            println!("{:#?}", value);

            let db_conf =
                build_database_conf(value.get("database").expect("Cannot see database").clone());
            println!("{:?}", db_conf);
            AetherConfig::default()
        }
        Err(e) => {
            log::error!("Failed to read file: {e}");
            exit(1);
        }
    }
}
pub fn generate_aether_config(config_file: Option<String>) -> AetherConfig {
    match &config_file {
        Some(value) => gen_aether_conf_from_config_file(value),
        None => AetherConfig::default(),
    }
}
