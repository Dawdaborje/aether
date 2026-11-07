use aether_web::routes::router as web_router;

use axum::{
    Router,
    routing::{get, post},
};

use aether_core::application::settings_api;
use aether_core::state::AppState;

/// Aggregate facet routers (web UI + settings). Auth is nested by the binary
/// to avoid a core ↔ authentication crate cycle.
pub fn routes() -> Router<AppState> {
    // `/web` serves the built app, or says clearly that it has not been built.
    let web = if aether_web::routes::is_built() {
        web_router()
    } else {
        log::warn!(
            "The web app has not been built (looked in {}); /web will explain how to build it",
            aether_web::routes::build_dir().display()
        );
        Router::new()
            .route("/web", get(aether_core::error_pages::web_app_missing))
            .route("/web/", get(aether_core::error_pages::web_app_missing))
            .route(
                "/web/{*rest}",
                get(aether_core::error_pages::web_app_missing),
            )
    };

    Router::new()
        .merge(web)
        .merge(crate::theme_api::routes())
        .merge(crate::apps_api::routes())
        .merge(crate::bridges_api::routes())
        .merge(crate::catalog_api::routes())
        .merge(crate::record_titles_api::routes())
        .merge(crate::plugin_assets::routes())
        .merge(aether_core::media_api::routes())
        .merge(crate::org_admin_api::routes())
        .merge(crate::scheduler_api::routes())
        .merge(crate::mantle_api::routes())
        .merge(crate::health_api::routes())
        .merge(crate::audit_api::routes())
        .merge(crate::search_favorites_api::routes())
        .merge(crate::record_favorites_api::routes())
        .merge(crate::viewers_api::routes())
        .merge(crate::packs_api::routes())
        .merge(aether_core::realtime::ws::routes())
        .merge(aether_core::sfu::routes())
        .merge(crate::api_tokens_api::routes())
        .merge(aether_core::webhooks_api::routes())
        .merge(aether_core::inbound::routes())
        .merge(crate::plugin_record_api::routes())
        .merge(crate::ai_chat::routes())
        .merge(crate::ui_prefs_api::routes())
        .merge(crate::company_api::routes())
        .merge(crate::models_api::routes())
        .merge(crate::demo_api::routes())
        .merge(aether_core::notifications::api::routes())
        .merge(aether_core::chatter::api::routes())
        .merge(aether_core::mail::api::routes())
        .merge(crate::wopi::routes())
        .merge(aether_core::bridge_credentials::routes())
        .route(
            "/api/plugins/{plugin}/{function}",
            post(aether_core::plugin_manager::api::invoke_plugin),
        )
        .merge(aether_core::pages_api::routes())
        .nest("/api/settings", settings_api::routes())
}
