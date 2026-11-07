pub mod ai_chat;
pub mod ai_tools;
pub mod api_tokens_api;
pub mod apps_api;
pub mod audit_api;
pub mod bridges_api;
pub mod catalog_api;
pub mod company_api;
pub mod demo_api;
pub mod docs_provider;
pub mod health_api;
pub mod history_retention;
pub mod mantle_api;
pub mod models_api;
pub mod org_admin_api;
pub mod plugin_activity;
pub mod plugin_assets;
pub mod plugin_commands;
pub mod plugin_record_api;
pub mod record_favorites_api;
pub mod record_titles_api;
pub mod routes;
pub mod scheduler_api;
#[cfg(feature = "schemars")]
pub mod schemas;
pub mod search_favorites;
pub mod search_favorites_api;
pub mod theme_api;
pub mod ui_prefs_api;
pub mod viewers_api;
pub mod packs_api;
pub mod wopi;

/// A plugin was installed, upgraded, removed, enabled or disabled in `org`: clear what the
/// kernel cached about its plugins (including the theme), then tell the organization's open
/// browsers with the transient event `kernel` / `plugins.changed`, so the app reloads its
/// theme and launcher without a manual refresh (`web/src/lib/apps/pluginsChanged.ts`).
pub fn plugins_changed(state: &aether_core::state::AppState, org: &str) {
    aether_core::plugin_manager::installed::invalidate(&state.cache, org);
    state.notifications.publish(
        org,
        aether_core::notifications::HubMessage::Event(std::sync::Arc::new(
            aether_core::notifications::UiEvent {
                plugin: "kernel".into(),
                event: "plugins.changed".into(),
                payload: serde_json::json!({}),
            },
        )),
    );
}
