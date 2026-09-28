use aether_web::routes::{api_router as web_api_router, router as web_router};

use axum::{
    Router,
    routing::{get, post},
};

use crate::application::settings_api;
use crate::state::AppState;

/// Aggregate facet routers (web UI + settings). Auth is nested by the binary
/// to avoid a core ↔ authentication crate cycle.
pub fn routes() -> Router<AppState> {
    Router::new()
        .nest("/web", web_router())
        .nest("/api/ui", web_api_router())
        .route(
            "/api/ui/ws/notifications",
            get(crate::websocket::notifications_ws),
        )
        .route(
            "/api/plugins/{plugin}/{function}",
            post(crate::plugin_manager::api::invoke_plugin),
        )
        .nest("/api/settings", settings_api::routes())
}
