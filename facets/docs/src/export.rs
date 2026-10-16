//! `aether --export-docs <dir>`: Aether's documentation as static files, for any web host (GitHub Pages, S3, nginx).
//!
//! The pages are rendered by the same router that serves them, so the copy cannot differ from the live site.
//! Every page is `<slug>/index.html`, so the links the site uses (`/architecture/models`) work on a host that
//! serves directory indexes. There is no search box, because searching needs the server.

use axum::{Router, body::Body, http::Request};
use http_body_util::BodyExt;
use std::io;
use std::path::Path;
use tower::ServiceExt;

/// Writes the site below `dir` (created if needed) for serving at `base` (`""` for the root of a host, or a
/// path such as `/aether-docs`). Returns the number of files written.
pub async fn export_site(dir: &Path, base: &str) -> io::Result<usize> {
    let base = base.trim().trim_matches('/');
    let base = if base.is_empty() { String::new() } else { format!("/{base}") };
    let app = crate::site::router_for_export(&base);
    let mut written = 0;

    let mut write = |relative: String, bytes: Vec<u8>| -> io::Result<()> {
        let target = dir.join(&relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target, bytes)?;
        written += 1;
        Ok(())
    };

    // Pages as HTML, and as the markdown they are written in.
    write("index.html".into(), get(&app, "/").await?)?;
    for d in &crate::bundle().docs {
        if d.slug != "index" {
            write(format!("{}/index.html", d.slug), get(&app, &format!("/{}", d.slug)).await?)?;
        }
        write(format!("{}.md", d.slug), get(&app, &format!("/{}.md", d.slug)).await?)?;
    }
    for file in ["llms.txt", "llms-full.txt", "assets/site.css", "assets/site.js"] {
        write(file.into(), get(&app, &format!("/{file}")).await?)?;
    }
    // What a host shows for an address that is not a page, and the marker that stops GitHub Pages from reading
    // the files as a Jekyll site.
    write("404.html".into(), get(&app, "/this-page-does-not-exist").await?)?;
    write(".nojekyll".into(), Vec::new())?;
    Ok(written)
}

async fn get(app: &Router, path: &str) -> io::Result<Vec<u8>> {
    let response = app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).map_err(io::Error::other)?)
        .await
        .map_err(io::Error::other)?;
    let bytes = response.into_body().collect().await.map_err(io::Error::other)?.to_bytes();
    Ok(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn exports_every_page_with_links_under_the_base() {
        let dir = tempfile::tempdir().unwrap();
        let count = export_site(dir.path(), "/aether-docs/").await.unwrap();
        let pages = crate::bundle().docs.len();
        assert_eq!(count, 1 + (pages - 1) + pages + 4 + 2, "index + pages + their markdown + 4 site files + 404 and .nojekyll");

        let read = |p: &str| std::fs::read_to_string(dir.path().join(p)).unwrap();
        let home = read("index.html");
        assert!(home.contains("href=\"/aether-docs/assets/site.css\"") && home.contains("Aether Official Documentation"), "{home}");
        let models = read("architecture/models/index.html");
        assert!(models.contains("<h1") && models.contains("href=\"/aether-docs/architecture/rules\""), "pages link to each other under the base");
        assert!(!models.contains("href=\"doc:") && !models.contains("href=\"/architecture"), "a link that ignores the base");
        assert!(!home.contains("action=\"/aether-docs/search\"") && !models.contains("type=\"search\""), "a static copy has no search box");
        assert!(read("architecture/models.md").starts_with("# Models"));
        assert!(read("llms.txt").contains("](/aether-docs/architecture/models.md)"));
        assert!(read("404.html").contains("Page not found"));
        assert!(read("assets/site.js").contains("theme"));
        assert!(dir.path().join(".nojekyll").exists());
    }

    #[tokio::test]
    async fn the_root_of_a_host_needs_no_base() {
        let dir = tempfile::tempdir().unwrap();
        export_site(dir.path(), "").await.unwrap();
        let page = std::fs::read_to_string(dir.path().join("plugin/pages/index.html")).unwrap();
        assert!(page.contains("href=\"/assets/site.css\"") && page.contains("href=\"/plugin/commands\""), "{page}");
    }
}
