use std::{
    io,
    path::{Path, PathBuf},
};

use tokio::fs;

const DEFAULT_CONFIG_TEMPLATE: &str = r#"app_dir = "app_dir"

[core]
instance_name = "Aether Test"

[server]
host = "0.0.0.0"
port = 7890
addon_paths = []

[configuration]
is_development_mode = false
is_development_with_assets = false

[database]
host = "localhost"
port = 8000
user = "root"
password = "root"
namespace = "aether"

[cache]
backend = "moka"
default_ttl_secs = 300
max_entries = 10000
max_value_bytes = 1048576

# [cache.redis]
# url = "redis://127.0.0.1:6379/0"
# key_prefix = "aether:cache:"

[tenancy]
org_resolution = "session_only"
org_header = "X-Org-Slug"
org_path_prefix = "/o"
"#;

const DEFAULT_PLUGIN_CONFIG_TEMPLATE: &str = r#"[plugin]
name = "{plugin_name}"
label = "{plugin_label}"
version = "0.0.1"
description = "{plugin_description}"
long_description = "{plugin_long_description}"
authors = [{ name = "Your Name", email = "your.email@example.com" }]
website = "https://your.website.com"
categories = []
dependencies = []
capabilities = ["db::query", "db::mutate"]

access_models = [
    { name = "user", permissions = ["read", "write"] },
]
is_builtin = false

# Contract version — bump only when breaking required keys.
[plugin.api]
version = "0.1"

[plugin.meta]
kind = "addon"
workspace = ""

[communication]
# Planned channels: events | email | sms | http | plugins
channels = []

"#;

const DEFAULT_PLUGIN_WORKSPACE_TEMPLATE: &str = r#"[workspace]
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
async fn write_file(file_path: PathBuf, content: &str) -> io::Result<()> {
    log::info!("Generating file at: {:?}", file_path);

    if let Some(parent) = file_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).await?;
        }
    }

    fs::write(&file_path, content).await
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

pub async fn generate_default_config_template(file_name: PathBuf) -> io::Result<()> {
    log::info!("File name: {:?}", file_name);

    write_file(file_name, DEFAULT_CONFIG_TEMPLATE).await
}

pub async fn generate_plugin_workspace(path: String) -> io::Result<()> {
    let workspace_path = PathBuf::from(path).join("workspace.toml");

    log::info!("Generating plugin workspace at: {:?}", workspace_path);

    write_file(workspace_path, DEFAULT_PLUGIN_WORKSPACE_TEMPLATE).await
}

pub async fn generate_plugin_config(path: String, name: &str) -> io::Result<()> {
    let root = PathBuf::from(&path);
    let config_path = root.join("plugin.toml");

    log::info!("Generating plugin config at: {:?}", config_path);

    write_file(config_path, &render_plugin_config(name)).await?;

    // Scaffold folders expected by the evolving plugin API (pages, hooks, …).
    for dir in ["models", "pages", "i18n"] {
        let dir_path = root.join(dir);
        fs::create_dir_all(&dir_path).await?;
    }

    write_file(
        root.join("hooks.toml"),
        "[[hook]]\nname = \"on_install\"\nphase = \"install\"\n",
    )
    .await?;
    write_file(
        root.join("permissions.toml"),
        &format!("[[permission]]\nkey = \"{name}.read\"\nlabel = \"Read {name}\"\n"),
    )
    .await?;
    write_file(root.join("events.toml"), "").await
}

fn find_extism_cli() -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path_var) {
        let candidate = directory.join("extism");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
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

async fn plugin_final_touches(name: String, project_path: PathBuf) -> io::Result<()> {
    // A workspace is optional; standalone plugins are valid projects too.
    if let Some(workspace_manifest) = find_workspace_manifest(&project_path) {
        log::info!(
            "Found optional plugin workspace at: {:?}",
            workspace_manifest
        );
    }

    // Generate the Aether plugin manifest inside the freshly scaffolded project.
    generate_plugin_config(project_path.to_string_lossy().to_string(), &name).await?;

    log::info!("Plugin project finalized at: {:?}", project_path);
    Ok(())
}

pub async fn create_plugin_project(path: String, name: String, language: String) -> io::Result<()> {
    let plugin_path = PathBuf::from(&path);
    let parent_path = plugin_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    log::info!("Creating plugin project at: {:?}", plugin_path);

    let Some(extism_cli) = find_extism_cli() else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Extism CLI is not installed. See https://github.com/extism/cli",
        ));
    };

    match std::process::Command::new(&extism_cli)
        .arg("--help")
        .output()
    {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            return Err(io::Error::other(format!(
                "Extism CLI at {:?} could not be executed: {}",
                extism_cli,
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Err(err) => {
            return Err(io::Error::other(format!(
                "Failed to execute Extism CLI at {:?}: {err}",
                extism_cli
            )));
        }
    }

    let arg_language = match language.to_lowercase().as_str() {
        "rust" => "rust",
        "python" => "python",
        "javascript" => "js",
        "typescript" => "ts",
        "go" => "go",
        unsupported => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "Unsupported plugin language '{unsupported}'. Choose go, rust, python, javascript, or typescript."
                ),
            ));
        }
    };

    // Create the destination's parent; Extism creates the project directory.
    fs::create_dir_all(parent_path).await?;

    let output = std::process::Command::new(&extism_cli)
        .arg("gen")
        .arg("plugin")
        .arg("-l")
        .arg(arg_language)
        .arg("-o")
        .arg(&plugin_path)
        .output()?;

    if !output.status.success() {
        return Err(io::Error::other(format!(
            "Failed to create plugin project: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    log::info!("Plugin project created successfully");

    plugin_final_touches(name, plugin_path).await
}
