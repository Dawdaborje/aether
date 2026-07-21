use std::path::PathBuf;
use std::process::exit;

use crate::plugin_manager::models::plugin_def::{PluginDefinition, PluginManifest};
use tokio::fs;

pub fn get_core_plugins() {}

/// Parse a `plugin.toml` into a full manifest (pages, hooks, events, …).
/// Unknown keys are ignored so the contract can grow without breaking older addons.
pub async fn load_plugin_manifest(file_path: PathBuf) -> Result<PluginManifest, String> {
    let file_content = fs::read_to_string(&file_path)
        .await
        .map_err(|err| format!("Failed to read {}: {err}", file_path.display()))?;

    let mut manifest: PluginManifest = toml::from_str(&file_content)
        .map_err(|err| format!("Failed to parse {}: {err}", file_path.display()))?;
    manifest.normalize();
    Ok(manifest)
}

pub async fn gen_plugins_from_conf(file_path: PathBuf) -> PluginDefinition {
    match load_plugin_manifest(file_path).await {
        Ok(manifest) => manifest.plugin,
        Err(err) => {
            log::error!("{err}");
            exit(1);
        }
    }
}

pub async fn get_plugins_from_config_file(file_paths: Vec<PathBuf>) -> Vec<PluginDefinition> {
    let mut plugins: Vec<PluginDefinition> = Vec::new();

    for file_path in file_paths {
        plugins.push(gen_plugins_from_conf(file_path).await);
    }

    plugins
}

pub async fn get_manifests_from_config_files(
    file_paths: Vec<PathBuf>,
) -> Vec<PluginManifest> {
    let mut manifests = Vec::new();
    for file_path in file_paths {
        match load_plugin_manifest(file_path).await {
            Ok(m) => manifests.push(m),
            Err(err) => {
                log::error!("{err}");
                exit(1);
            }
        }
    }
    manifests
}

pub async fn reload_plugins(_plugins: Vec<PluginDefinition>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_forward_compatible_manifest() {
        let toml = r#"
[plugin]
name = "partner"
label = "Partners"
version = "0.1.0"
dependencies = ["currency"]
capabilities = ["db::query"]
is_builtin = true

[plugin.api]
version = "0.1"

[plugin.meta]
kind = "addon"
workspace = "base"

[communication]
channels = ["events"]

[[pages]]
route = "/base/partners"
title = "Contacts"
file = "./pages/partners.xml"

[[hooks]]
name = "on_install"
phase = "install"
handler = "hooks.on_install"

# Future keys must not break loaders
[plugin.future_thing]
enabled = true
"#;
        let mut manifest: PluginManifest = toml::from_str(toml).expect("parse");
        manifest.normalize();
        assert_eq!(manifest.plugin.name, "partner");
        assert_eq!(manifest.plugin.api_version(), "0.1");
        assert_eq!(manifest.plugin.kind.as_deref(), Some("addon"));
        assert_eq!(manifest.plugin.workspace.as_deref(), Some("base"));
        assert_eq!(manifest.pages.len(), 1);
        assert_eq!(manifest.hooks.len(), 1);
        assert_eq!(manifest.communication.channels, vec!["events"]);
    }
}
