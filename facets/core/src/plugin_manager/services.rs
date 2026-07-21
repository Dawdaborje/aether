use std::{path::PathBuf, process::exit, str::FromStr};

use crate::plugin_manager::models::plugin_def::PluginDefinition;
use tokio::fs;
use toml::Value;

pub fn get_core_plugins() {}

pub async fn gen_plugins_from_conf(file_path: PathBuf) -> PluginDefinition {
    match fs::read_to_string(file_path).await {
        Ok(file_content) => {
            let value = Value::from_str(&file_content).expect("Error parsing the config file");

            if let Value::Table(plugin_table) =
                value.get("plugin").expect("Failed to get the value plugin")
            {
                PluginDefinition {
                    name: plugin_table["name"]
                        .as_str()
                        .expect("Failed to get the key name ")
                        .to_string(),
                    label: plugin_table["label"].as_str().expect("Failed").to_string(),
                    version: plugin_table["version"]
                        .as_str()
                        .expect("Failed")
                        .to_string(),
                    description: Some(plugin_table["descriptiopn"]
                        .as_str()
                        .expect("Failed")
                        .to_string()),
                    long_description: Some(plugin_table["long_description"]
                        .as_str()
                        .expect("Failed")
                        .to_string()),
                    icon_path: Some(plugin_table["icon_path"]
                        .as_str()
                        .expect("Failed")
                        .to_string()),
                    ..Default::default()
                }
            } else {
                log::error!("Plugin table must be defined");
                exit(1);
            }
        }
        Err(err) => {
            log::error!("Failed to read file: {err}");
            exit(1);
        }
    }
}

pub async fn get_plugins_from_config_file(file_paths: Vec<PathBuf>) -> Vec<PluginDefinition> {
    let mut plugins: Vec<PluginDefinition> = Vec::new();

    for file_path in file_paths {
        plugins.push(gen_plugins_from_conf(file_path).await)
    }

    plugins
}


pub async fn reload_plugins(plugins: Vec<PluginDefinition>) {
    
}