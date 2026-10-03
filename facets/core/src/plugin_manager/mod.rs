pub mod access;
pub mod api;
pub mod catalog;
pub mod models;
pub mod pages;
pub mod runtime;
pub mod themes;
pub mod services;

pub fn get_core_plugins() -> Vec<models::plugin_def::PluginDefinition> {
    // const CORE_PLUGIN_PATH: &str = "../../../../addons";

    vec![]
}
