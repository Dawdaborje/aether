//! Part of the plugin catalog; see [`super`].

use super::*;

#[derive(Debug, Clone, Serialize, SurrealValue)]
pub(super) struct NewPlugin {
    pub(super) name: String,
    pub(super) label: String,
    pub(super) version: String,
    pub(super) description: Option<String>,
    pub(super) long_description: Option<String>,
    pub(super) icon_path: Option<String>,
    pub(super) website: Option<String>,
    pub(super) authors: Vec<PluginDbAuthor>,
    pub(super) categories: Vec<PluginDbCategory>,
    pub(super) dependencies: Vec<String>,
    pub(super) workspace: Option<String>,
    pub(super) kind: Option<String>,
    pub(super) artifact_path: Option<String>,
    pub(super) artifact_hash: Option<String>,
    pub(super) content_hash: String,
    /// Hash of every file in the package folder as it was when this row was written. A later
    /// load whose folder hashes the same has nothing new and skips parsing and validation.
    pub(super) source_hash: Option<String>,
    /// The folder under `plugins/<name>/` holding this version's file index.
    pub(super) revision: Option<String>,
    /// Where the package was loaded from.
    pub(super) source_path: Option<String>,
    /// The launcher tile, when the plugin is an app.
    pub(super) app: Option<NewApp>,
    /// Functions anonymous visitors may call; checked before a module is compiled.
    pub(super) public_functions: Vec<String>,
    /// The manifest's `[[schedule]]` entries.
    pub(super) schedules: Vec<serde_json::Value>,
    /// The manifest's `[[command]]` entries.
    pub(super) commands: Vec<serde_json::Value>,
    /// The plugin's translated text, from `i18n/<locale>.json`.
    pub(super) i18n: Option<serde_json::Value>,
    /// The manifest's `[[watch]]` entries.
    pub(super) watches: Vec<serde_json::Value>,
    /// The events the manifest listens to.
    pub(super) event_listeners: Vec<serde_json::Value>,
    /// The manifest's `[[roles]]`.
    pub(super) roles: Vec<serde_json::Value>,
    /// The manifest's `[[ai_tool]]` entries.
    pub(super) ai_tools: Vec<serde_json::Value>,
    /// The plugin's rendered documentation (`[docs]`), see `plugin_manager::docs`.
    pub(super) docs: Option<serde_json::Value>,
    pub(super) is_builtin: bool,
    pub(super) is_active: bool,
}

#[derive(Debug, Clone, Serialize, SurrealValue)]
pub(super) struct NewApp {
    pub(super) label: String,
    pub(super) icon: Option<String>,
    pub(super) route: String,
    pub(super) description: Option<String>,
    #[serde(default)]
    pub(super) standalone: bool,
    #[serde(default)]
    pub(super) also_in_host: bool,
    #[serde(default)]
    pub(super) folder: Option<String>,
    #[serde(default)]
    pub(super) roles: Vec<String>,
    #[serde(default)]
    pub(super) nav: bool,
    /// The checked `[[app.palette]]` entries (`read::palette_entries`).
    #[serde(default)]
    pub(super) palette: Vec<serde_json::Value>,
    /// The checked `[app.tile]` (`read::tile_entry`).
    #[serde(default)]
    pub(super) tile: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, SurrealValue)]
pub(super) struct NewModelRow {
    pub(super) name: String,
    pub(super) model_id: String,
    pub(super) label: Option<String>,
    pub(super) definition: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, SurrealValue)]
pub(super) struct NewTheme {
    pub(super) name: String,
    pub(super) label: String,
    pub(super) is_system: bool,
    pub(super) color_mode: String,
    pub(super) tokens: serde_json::Value,
    pub(super) layout: String,
    pub(super) error_pages: String,
    pub(super) nav: Option<serde_json::Value>,
    pub(super) layout_def: Option<serde_json::Value>,
    pub(super) error_pages_def: Option<serde_json::Value>,
    pub(super) shell: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, SurrealValue)]
pub(super) struct NewLayoutRow {
    pub(super) name: String,
    pub(super) label: String,
    pub(super) chrome: String,
    pub(super) definition: serde_json::Value,
    pub(super) source_path: String,
}

#[derive(Debug, Clone, Serialize, SurrealValue)]
pub(super) struct NewErrorSetRow {
    pub(super) name: String,
    pub(super) label: String,
    pub(super) variant: String,
    pub(super) definition: serde_json::Value,
    pub(super) source_path: String,
}

#[derive(Debug, Clone, Serialize, SurrealValue)]
pub(super) struct NewUiPage {
    pub(super) layout: Option<String>,
    pub(super) wrap: Option<String>,
    pub(super) route: String,
    pub(super) is_pattern: bool,
    pub(super) route_shape: String,
    pub(super) title: String,
    pub(super) model: Option<String>,
    pub(super) models: Vec<String>,
    pub(super) is_public: bool,
    pub(super) is_menu_visible: bool,
    pub(super) icon: Option<String>,
    pub(super) menu_group: Option<String>,
    pub(super) menu_group_icon: Option<String>,
    pub(super) sort_order: i64,
    pub(super) required_roles: Vec<String>,
    pub(super) component_tree: serde_json::Value,
    pub(super) source_path: String,
}

#[derive(Debug, Clone, Serialize, SurrealValue)]
pub(super) struct NewUiComponent {
    pub(super) name: String,
    pub(super) full_name: String,
    pub(super) component_tree: serde_json::Value,
    pub(super) source_path: String,
}

#[derive(Debug, Deserialize, SurrealValue)]
pub(super) struct KnownVersion {
    pub(super) version: String,
    pub(super) content_hash: Option<String>,
    pub(super) revision: Option<String>,
}

#[derive(Debug, Deserialize, SurrealValue)]
pub(super) struct ModelIdOwner {
    pub(super) model_id: String,
    pub(super) plugin: String,
}

#[derive(Debug, Deserialize, SurrealValue)]
pub(super) struct InstalledRow {
    pub(super) plugin_name: String,
    pub(super) version: String,
}

#[derive(Debug, Deserialize, SurrealValue)]
pub(super) struct OrgDatabaseRow {
    #[allow(dead_code)]
    pub(super) db_name: String,
}

/// The WASM artifact: where it is in the package, and the file name it gets
/// at the root of the plugin's `app_dir` folder.
pub(super) struct Artifact {
    pub(super) source: PathBuf,
    pub(super) name: PathBuf,
}

/// A theme added to an organization by installing its plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledTheme {
    pub name: String,
    pub layout: String,
    /// `true` when it became the organization's active theme (the first theme
    /// an organization installs does).
    pub activated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub(super) struct PlannedTheme {
    pub(super) plugin: surrealdb::types::RecordId,
    pub(super) name: String,
    pub(super) label: String,
    pub(super) is_system: bool,
    pub(super) color_mode: String,
    pub(super) tokens: serde_json::Value,
    pub(super) layout: String,
    pub(super) error_pages: String,
    pub(super) nav: Option<serde_json::Value>,
    pub(super) layout_def: Option<serde_json::Value>,
    pub(super) error_pages_def: Option<serde_json::Value>,
    pub(super) shell: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, SurrealValue)]
pub(super) struct PlannedModel {
    pub(super) plugin: surrealdb::types::RecordId,
    pub(super) definition: serde_json::Value,
}

#[derive(Debug, Deserialize, SurrealValue)]
pub(super) struct ThemeColorMode {
    pub(super) color_mode: String,
}

#[derive(Debug, Deserialize, SurrealValue)]
pub(super) struct PlannedPageRoute {
    pub(super) route: String,
    pub(super) route_shape: String,
    pub(super) plugin: surrealdb::types::RecordId,
}

#[derive(Debug, Deserialize, SurrealValue)]
pub(super) struct ClaimedRoute {
    pub(super) route: String,
    pub(super) route_shape: String,
    pub(super) plugin_name: String,
    pub(super) plugin_version: String,
}

#[derive(Debug, Deserialize, SurrealValue)]
pub(super) struct TreeOwnerRow {
    pub(super) component_tree: serde_json::Value,
    pub(super) plugin_name: String,
    pub(super) plugin_version: String,
}

#[derive(Debug, Deserialize, SurrealValue)]
pub(super) struct ForeignComponentRow {
    pub(super) full_name: String,
    pub(super) component_tree: serde_json::Value,
}
