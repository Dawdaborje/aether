use axum::Router;
use std::path::PathBuf;
use tower_http::services::{ServeDir, ServeFile};

use crate::api;

/// Static SPA. Nested by core at `/web`.
pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let build_dir = resolve_web_build_dir();
    let fallback = build_dir.join("200.html");

    Router::new().fallback_service(
        ServeDir::new(&build_dir)
            .append_index_html_on_directories(true)
            .fallback(ServeFile::new(fallback)),
    )
}

/// UI REST API (theme, pages). Nested by core at `/api/ui`.
pub fn api_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    api::router()
}

fn resolve_web_build_dir() -> PathBuf {
    if let Ok(path) = std::env::var("AETHER_WEB_BUILD") {
        return PathBuf::from(path);
    }

    let from_crate = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../aether_web/build");
    if from_crate.exists() {
        return from_crate;
    }

    for candidate in ["aether_web/build", "../aether_web/build", "../../aether_web/build"] {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return path;
        }
    }

    from_crate
}
