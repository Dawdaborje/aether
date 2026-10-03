use aether_web::routes::router as web_router;

use axum::{
    Router,
    routing::{get, post},
};

use crate::application::settings_api;
use crate::state::AppState;

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
            .route("/web", get(crate::error_pages::web_app_missing))
            .route("/web/", get(crate::error_pages::web_app_missing))
            .route("/web/{*rest}", get(crate::error_pages::web_app_missing))
    };

    Router::new()
        .merge(web)
        .merge(crate::theme_api::routes())
        .merge(crate::apps_api::routes())
        .merge(crate::bridges_api::routes())
        .merge(crate::catalog_api::routes())
        .merge(crate::org_admin_api::routes())
        .merge(crate::notifications::api::routes())
        .route(
            "/api/plugins/{plugin}/{function}",
            post(crate::plugin_manager::api::invoke_plugin),
        )
        .merge(crate::pages_api::routes())
        .nest("/api/settings", settings_api::routes())
}
