use aether_web::routes::{api_router as web_api_router, router as web_router};
use axum::Router;

use crate::application::settings_api;
use crate::state::AppState;

/// Aggregate facet routers (web UI + settings). Auth is nested by the binary
/// to avoid a core ↔ authentication crate cycle.
pub fn routes() -> Router<AppState> {
    Router::new()
        .nest("/web", web_router())
        .nest("/api/ui", web_api_router())
        .nest("/api/settings", settings_api::routes())
}
