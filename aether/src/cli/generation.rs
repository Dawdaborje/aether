use std::{
    path::{Path, PathBuf},
    process::exit,
};

use tokio::fs;

const DEFAULT_CONFIG_TEMPLATE: &str = 
r#"[core]
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

const DEFAULT_PLUGIN_CONFIG_TEMPLATE: &str = 
r#"[plugin]
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
capabilities = ["db::query", "db::mutate"]
access_models = [
    { name = "user", permissions = ["read", "write"]},
]

"#;

const DEFAULT_PLUGIN_WORKSPACE_TEMPLATE: &str = 
r#"[workspace]
name = "example"
label = "Example Plugin"
version = "0.0.1"
description = "This is a short description of the plugin workspace."
long_description = "This is a long description of the plugin workspace."
authors = [{name = "Your Name", email = "your.email@example.com"}]
website = "https://your.website.com"
categories = []
dependencies = []

# Register each addon plugin directory here.
# [[addons]]
# name = "my_addon"
# path = "./my_addon"
"#;

/// Writes `content` to `file_path`, creating parent directories as needed.
async fn write_file(file_path: PathBuf, content: &str) {
    log::info!("Generating file at: {:?}", file_path);

    if let Some(parent) = file_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .await
                .expect("Failed to create parent directories");
        }
    }

    fs::write(&file_path, content).await.unwrap_or_else(|err| {
        log::error!("Failed to write file {:?}: {err}", file_path);
        exit(1);
    });
}

/// Turns a raw plugin name into a human friendly label, e.g. `my_plugin` -> `My Plugin`.
fn humanize(name: &str) -> String {
    name.split(['_', ' ', '-'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Fills the plugin config template with the given plugin name and derived label.
fn render_plugin_config(name: &str) -> String {
    let label = humanize(name);

    DEFAULT_PLUGIN_CONFIG_TEMPLATE
        .replace("{plugin_name}", name)
        .replace("{plugin_label}", &label)
        .replace("{plugin_description}", "A short description of the plugin.")
        .replace(
            "{plugin_long_description}",
            "A longer description of what the plugin does.",
        )
}

pub async fn generate_default_config_template(file_name: PathBuf) {
    log::info!("File name: {:?}", file_name);

    write_file(file_name, DEFAULT_CONFIG_TEMPLATE).await;
}

pub async fn generate_plugin_workspace(path: String) {
    let workspace_path = PathBuf::from(path).join("workspace.toml");

    log::info!("Generating plugin workspace at: {:?}", workspace_path);

    write_file(workspace_path, DEFAULT_PLUGIN_WORKSPACE_TEMPLATE).await;
}

pub async fn generate_plugin_config(path: String, name: &str) {
    let config_path = PathBuf::from(path).join("config.toml");

    log::info!("Generating plugin config at: {:?}", config_path);

    write_file(config_path, &render_plugin_config(name)).await;
}

fn check_if_extism_cli_is_installed() -> bool {
    match std::process::Command::new("extism").arg("version").output() {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

/// Walks up from `start` looking for a `workspace.toml` in an ancestor directory.
fn find_workspace_manifest(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);

    while let Some(dir) = current {
        let candidate = dir.join("workspace.toml");
        if candidate.exists() {
            return Some(candidate);
        }
        current = dir.parent();
    }

    None
}

async fn plugin_final_touches(name: String, project_path: PathBuf) {
    // A plugin must live inside a workspace, so make sure one exists above it.
    match find_workspace_manifest(&project_path) {
        Some(workspace_manifest) => {
            log::info!("Found plugin workspace at: {:?}", workspace_manifest);
        }
        None => {
            log::error!(
                "No workspace.toml found in any parent directory of {:?}. \
                 Run `aether --gen workspace` first.",
                project_path
            );
            exit(1);
        }
    }

    // Generate the Aether plugin manifest inside the freshly scaffolded project.
    generate_plugin_config(project_path.to_string_lossy().to_string(), &name).await;

    log::info!("Plugin project finalized at: {:?}", project_path);
}

pub async fn create_plugin_project(path: String, name: String, language: String) {
    let plugin_path = PathBuf::from(&path);

    log::info!("Creating plugin project at: {:?}", plugin_path);

    if !check_if_extism_cli_is_installed() {
        log::error!("Extism CLI is not installed. See https://github.com/extism/cli");
        return;
    }

    let arg_language = match language.to_lowercase().as_str() {
        "rust" => "rust",
        "python" => "python",
        "javascript" => "js",
        "typescript" => "ts",
        _ => "go",
    };
    let project_id = name.to_lowercase().replace(' ', "_"); // separate spaces with underscores

    // Make sure the destination directory exists so extism can generate into it.
    fs::create_dir_all(&plugin_path)
        .await
        .expect("Failed to create the plugin directory");

    let output = std::process::Command::new("extism")
        .arg("gen")
        .arg("plugin")
        .arg("-l")
        .arg(arg_language)
        .arg(&project_id)
        .current_dir(&plugin_path)
        .output()
        .expect("Failed to create plugin project");

    if !output.status.success() {
        log::error!(
            "Failed to create plugin project: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }

    log::info!("Plugin project created successfully");

    let project_path = plugin_path.join(&project_id);
    plugin_final_touches(name, project_path).await;
}
