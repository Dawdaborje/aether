use axum::Router;
use std::path::PathBuf;
use tower_http::services::{ServeDir, ServeFile};

/// The single-page app's entry file inside the build directory.
const ENTRY: &str = "200.html";

/// Where the built web app is, and whether it has been built.
pub fn build_dir() -> PathBuf {
    resolve_web_build_dir()
}

/// `true` when `web/build` holds a built app (`pnpm build` in `web/`).
pub fn is_built() -> bool {
    resolve_web_build_dir().join(ENTRY).is_file()
}

/// The built web app, mounted at `/web`.
///
/// Every path that is not a file in the build (including `/web`, `/web/` and
/// unknown routes) answers with the app's entry file, so the app's own router
/// decides what to show. Looking for an `index.html` in directories is turned
/// off: the build has none, and with it on a request for the directory itself
/// (`/web/`) is a 404 instead of falling back to the entry file.
pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let build_dir = resolve_web_build_dir();
    let entry = build_dir.join(ENTRY);

    Router::new().nest_service(
        "/web",
        ServeDir::new(&build_dir)
            .append_index_html_on_directories(false)
            .fallback(ServeFile::new(entry)),
    )
}

fn resolve_web_build_dir() -> PathBuf {
    if let Ok(path) = std::env::var("AETHER_WEB_BUILD") {
        return PathBuf::from(path);
    }

    let from_crate = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../web/build");
    if from_crate.exists() {
        return from_crate;
    }

    for candidate in ["web/build", "../web/build", "../../web/build"] {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return path;
        }
    }

    from_crate
}
