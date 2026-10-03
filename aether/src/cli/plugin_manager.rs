use std::path::{Path, PathBuf};

use aether_core::plugin_manager::catalog::{
    self, CatalogError, InstallReport, LoadedPlugin, PluginSpec, UpgradeReport,
};
use surrealdb::{Surreal, engine::remote::ws::Client};

/// Register each plugin package in the core catalog, stopping at the first failure.
pub async fn load_plugins(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    app_dir: &Path,
    plugin_paths: &[PathBuf],
) -> Result<Vec<LoadedPlugin>, CatalogError> {
    let mut loaded = Vec::with_capacity(plugin_paths.len());
    for plugin_path in plugin_paths {
        loaded
            .push(catalog::load_plugin(db, namespace, core_database, app_dir, plugin_path).await?);
    }
    Ok(loaded)
}

/// Install catalog plugins (with their dependencies) for one organization.
pub async fn install_plugins(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    org_database: &str,
    specs: &[PluginSpec],
) -> Result<InstallReport, CatalogError> {
    catalog::install_plugins(db, namespace, core_database, org_database, specs).await
}

/// Move an organization's plugins to another catalog version.
pub async fn upgrade_plugins(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    org_database: &str,
    specs: &[PluginSpec],
) -> Result<UpgradeReport, CatalogError> {
    catalog::upgrade_plugins(db, namespace, core_database, org_database, specs).await
}

pub fn print_upgrade_summary(org_database: &str, report: &UpgradeReport) {
    for (name, from, to) in &report.upgraded {
        println!("Upgraded plugin '{name}' for organization '{org_database}': {from} -> {to}.");
    }
    for (name, version) in &report.already_current {
        println!("Plugin '{name}' is already on '{version}' for organization '{org_database}'.");
    }
}

pub fn print_load_summary(loaded: &[LoadedPlugin]) {
    for plugin in loaded {
        let status = if plugin.created {
            "Loaded"
        } else {
            "Already loaded"
        };
        let artifact = plugin
            .artifact_path
            .as_deref()
            .map_or_else(|| "no WASM artifact".to_string(), |path| format!("artifact: {path}"));
        // What was written, when something was: only the files whose hash changed.
        let files = match (&plugin.revision, plugin.created) {
            (Some(revision), true) if plugin.reused_files > 0 => format!(
                "; revision {revision}: {} changed file(s) stored, {} unchanged reused",
                plugin.written_files, plugin.reused_files
            ),
            (Some(revision), true) => {
                format!("; revision {revision}: {} file(s) stored", plugin.written_files)
            }
            _ => String::new(),
        };
        println!(
            "{status} plugin '{}@{}' ({artifact}; {} page(s), {} public{}{}{files}).",
            plugin.name,
            plugin.version,
            plugin.pages,
            plugin.public_pages,
            plugin
                .theme
                .as_ref()
                .map_or(String::new(), |(name, layout)| format!("; theme '{name}', layout '{layout}'")),
            plugin
                .app
                .as_ref()
                .map_or(String::new(), |(label, route)| format!("; app '{label}' opening {route}"))
        );
    }
}

/// Make an installed theme the organization's active theme.
pub async fn activate_theme(
    db: &Surreal<Client>,
    namespace: &str,
    org_database: &str,
    theme_name: &str,
) -> Result<(), CatalogError> {
    catalog::activate_theme(db, namespace, org_database, theme_name).await
}

pub fn print_install_summary(org_database: &str, report: &InstallReport) {
    for (name, version) in &report.installed {
        println!("Installed plugin '{name}@{version}' for organization '{org_database}'.");
    }
    for theme in &report.themes {
        if theme.activated {
            println!(
                "Theme '{}' (layout '{}') is now the active theme for '{org_database}'.",
                theme.name, theme.layout
            );
        } else {
            println!(
                "Theme '{}' (layout '{}') added to '{org_database}'. Switch to it with --activate-theme {} --org {org_database}.",
                theme.name, theme.layout, theme.name
            );
        }
    }
    for (name, version) in &report.already_installed {
        println!(
            "Plugin '{name}@{version}' is already installed for organization '{org_database}'."
        );
    }
}
