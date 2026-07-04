use std::path::PathBuf;

use tokio::fs;

const DEFAULT_CONFIG_TEMPLATE: &str = r#"
[core]
instance_name = "Aether Test"

[server]
port = 7890
addon_paths = []

[database]
host = "localhost"
port = 8000
user = "root"
password = "root"
namespace = "aether"
"#;

pub async fn generate_default_config_template(file_name: PathBuf) {
    log::info!("File name: {:?}", file_name);

    fs::write(file_name, DEFAULT_CONFIG_TEMPLATE)
        .await
        .expect("Failed to create a config file");
}
