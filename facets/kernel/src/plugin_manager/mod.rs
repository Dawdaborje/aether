pub mod access;
pub mod catalog;
pub mod components;
pub mod docs;
pub mod exports;
pub mod field_hints;
pub mod i18n;
pub mod installed;
pub mod layout_blocks;
pub mod layout_xml;
pub mod screens;
pub mod load;
pub mod load_paths;
pub mod model_edit;
pub mod mantle_signals;
pub mod models;
pub mod page_patch;
pub mod pages;
pub mod openapi;
pub mod record_actions;
pub mod revisions;
pub mod runtime;
pub mod script;
pub mod services;
pub mod shell_defs;
pub mod surface;
pub mod theme_css;
pub mod theme_extras;
pub mod theme_icons;
pub mod themes;
pub mod uninstall;
pub mod workspaces;
pub mod xml_tree;

pub fn get_core_plugins() -> Vec<models::plugin_def::PluginDefinition> {
    // const CORE_PLUGIN_PATH: &str = "../../../../addons";

    vec![]
}
