use aether_authentication::routes::routes as auth_routes;
use aether_web::routes::router as web_router;
use axum::Router;

pub fn routes() -> axum::Router {
    Router::new()
        .nest("/web", web_router())
        .nest("/api/auth", auth_routes())
}
