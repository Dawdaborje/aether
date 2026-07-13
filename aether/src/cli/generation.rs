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

const DEFAULT_PLUGIN_CONFIG_TEMPLATE: &str = r#"
[plugin]
name = "{plugin_name}"
label = "{plugin_label}"
version = "0.0.1"
description = "{plugin_description}"
long_description = "{plugin_long_description}"
authors = [{name = "Your Name", email = "your.email@example.com"}]
website = "https://your.website.com"
categories = []
dependencies = []

#security
capabilities = ["db:read", "db:write"]
access_models = [
    { name = "user", permissions = ["read", "write"]},
]

"#;

const DEFAULT_PLUGIN_WORKSPACE_TEMPLATE: &str = r#"
[workspace]
name = "{plugin_name}"
label = "{plugin_label}"
version = "0.0.1"
description = "This is a short description of the plugin workspace."
long_description = "This is a long description of the plugin workspace."
authors = [{name = "Your Name", email = "your.email@example.com"}]
website = "https://your.website.com"
categories = []
dependencies = []
"#;



pub async fn generate_default_config_template(file_name: PathBuf) {
    log::info!("File name: {:?}", file_name);

    fs::write(file_name, DEFAULT_CONFIG_TEMPLATE)
        .await
        .expect("Failed to create a config file");
}

pub async fn generate_plugin_workspace(path: String) {
    let workspace_path = PathBuf::from(path).join("workspace.toml");

    log::info!("Generating plugin workspace at: {:?}", workspace_path);

    fs::write(&workspace_path, DEFAULT_PLUGIN_WORKSPACE_TEMPLATE)
        .await
        .expect("Failed to create a workspace file");
}