pub mod access;
pub mod api;
pub mod catalog;
pub mod commands;
pub mod components;
pub mod exports;
pub mod field_hints;
pub mod i18n;
pub mod installed;
pub mod layout_blocks;
pub mod load;
pub mod load_paths;
pub mod mantle_signals;
pub mod model_edit;
pub mod models;
pub mod page_patch;
pub mod pages;
pub mod revisions;
pub mod runtime;
pub mod script;
pub mod services;
pub mod shell_defs;
pub mod surface;
pub mod theme_css;
pub mod themes;
pub mod workspaces;
pub mod xml_tree;

pub fn get_core_plugins() -> Vec<models::plugin_def::PluginDefinition> {
    // const CORE_PLUGIN_PATH: &str = "../../../../addons";

    vec![]
}
