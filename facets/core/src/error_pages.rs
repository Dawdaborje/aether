//! Error responses for requests that never reach the web app: unknown URLs,
//! and `/web` before the app has been built.
//!
//! API clients (`/api/...`, or anything that does not ask for HTML) get JSON;
//! browsers get a small self-contained page. The web app has its own,
//! themeable, error pages (`web/src/lib/components/errors`); this is the
//! default for everything outside it.

use axum::{
    Json,
    http::{HeaderMap, StatusCode, Uri, header},
    response::{Html, IntoResponse, Response},
};
use serde_json::json;

/// Escape text for placing in HTML.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// A complete HTML error page. `detail` is shown under the message; it is
/// escaped here, so callers may pass request data such as the path.
pub fn error_page_html(status: StatusCode, title: &str, message: &str, detail: Option<&str>) -> String {
    let detail = detail
        .map(|detail| format!("<p class=\"detail\"><code>{}</code></p>", escape(detail)))
        .unwrap_or_default();
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{code} {title}</title>
<style>
  :root {{ color-scheme: light dark; }}
  body {{ margin: 0; min-height: 100vh; display: grid; place-items: center;
          font: 16px/1.5 system-ui, sans-serif; background: Canvas; color: CanvasText; }}
  main {{ max-width: 32rem; padding: 2rem; }}
  .code {{ font-size: 4rem; font-weight: 700; margin: 0; opacity: .85; }}
  h1 {{ font-size: 1.25rem; margin: .25rem 0 .5rem; }}
  p {{ margin: .25rem 0; opacity: .8; }}
  .detail code {{ font: .9em ui-monospace, monospace; word-break: break-all; }}
  a {{ color: inherit; }}
</style>
</head>
<body>
<main>
  <p class="code">{code}</p>
  <h1>{title}</h1>
  <p>{message}</p>
  {detail}
  <p><a href="/web">Go to the app</a></p>
</main>
</body>
</html>"#,
        code = status.as_u16(),
        title = escape(title),
        message = escape(message),
    )
}

fn wants_html(uri: &Uri, headers: &HeaderMap) -> bool {
    !uri.path().starts_with("/api/")
        && headers
            .get(header::ACCEPT)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|accept| accept.contains("text/html"))
}

/// The router's fallback: nothing matched this URL.
pub async fn not_found(uri: Uri, headers: HeaderMap) -> Response {
    if wants_html(&uri, &headers) {
        (
            StatusCode::NOT_FOUND,
            Html(error_page_html(
                StatusCode::NOT_FOUND,
                "Not found",
                "There is nothing at this address.",
                Some(uri.path()),
            )),
        )
            .into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "not found", "path": uri.path() })),
        )
            .into_response()
    }
}

/// `/web` when the web app has not been built.
pub async fn web_app_missing() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Html(error_page_html(
            StatusCode::SERVICE_UNAVAILABLE,
            "The web app has not been built",
            "Aether is running, but there is no built web app to serve. Build it with `pnpm build` in the web/ folder, or point AETHER_WEB_BUILD at a build.",
            None,
        )),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_escapes_request_data() {
        let html = error_page_html(StatusCode::NOT_FOUND, "Not found", "m", Some("/<script>alert(1)</script>"));
        assert!(html.contains("404"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!html.contains("<script>alert"));
    }

    #[test]
    fn browsers_get_html_and_api_clients_get_json() {
        let mut browser = HeaderMap::new();
        browser.insert(header::ACCEPT, header::HeaderValue::from_static("text/html,*/*"));
        assert!(wants_html(&Uri::from_static("/nope"), &browser));
        assert!(!wants_html(&Uri::from_static("/api/nope"), &browser), "API paths are always JSON");
        assert!(!wants_html(&Uri::from_static("/nope"), &HeaderMap::new()));
    }
}
