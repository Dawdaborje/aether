use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

pub fn router() -> Router {
    Router::new().nest_service(
        "/web",
        ServeDir::new("../../../aether_web/build")
            .append_index_html_on_directories(true)
            .precompressed_gzip()
            // Dynamic client-side routes (e.g. org slugs) are not prerendered,
            // so serve the SPA fallback page for any unmatched path.
            .fallback(ServeFile::new("../../../aether_web/build/200.html")),
    )
}
