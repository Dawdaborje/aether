pub mod access;
pub mod api;
pub mod catalog;
pub mod commands;
pub mod components;
pub mod i18n;
pub mod load_paths;
pub mod model_edit;
pub mod models;
pub mod pages;
pub mod revisions;
pub mod runtime;
pub mod script;
pub mod shell_defs;
pub mod themes;
pub mod services;
pub mod xml_tree;

pub fn get_core_plugins() -> Vec<models::plugin_def::PluginDefinition> {
    // const CORE_PLUGIN_PATH: &str = "../../../../addons";

    vec![]
}
