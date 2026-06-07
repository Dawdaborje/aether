use axum::Router;
use tower_http::services::ServeDir;

pub fn router() -> Router {
    Router::new().nest_service(
        "/web",
        ServeDir::new("../../../aether_web/build")
            .append_index_html_on_directories(true)
            .precompressed_gzip(),
    )
}
