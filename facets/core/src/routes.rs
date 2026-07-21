use aether_authentication::routes::routes as auth_routes;
use aether_web::routes::{api_router as web_api_router, router as web_router};
use axum::Router;

/// Aggregate facet routers. The aether binary should nest/merge only this.
pub fn routes() -> Router {
    Router::new()
        .nest("/web", web_router())
        .nest("/api/ui", web_api_router())
        .nest("/api/auth", auth_routes())
}
