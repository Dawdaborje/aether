//! Security headers on every response.
//!
//! A header a handler already set is left alone, so the stricter policies of the file-serving
//! endpoints (a sandbox for uploads, say) win. The policy here is deliberately narrow: it forbids
//! being framed by another site, changing the base URL and posting forms elsewhere, which cannot
//! break the API, the documentation site or a web app served from another origin. A full
//! script policy belongs to whoever serves the web app.

use axum::{
    extract::{Request, State},
    http::{HeaderName, HeaderValue, header},
    middleware::Next,
    response::Response,
};

#[derive(Clone, Copy)]
pub struct HeaderPolicy {
    /// Browsers may only be told to insist on HTTPS once the site really is served over it, so
    /// this is off in development mode.
    pub hsts: bool,
}

const FRAME_OPTIONS: HeaderName = HeaderName::from_static("x-frame-options");

fn set_if_absent(response: &mut Response, name: HeaderName, value: &'static str) {
    response
        .headers_mut()
        .entry(name)
        .or_insert(HeaderValue::from_static(value));
}

pub async fn add_security_headers(State(policy): State<HeaderPolicy>, request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    set_if_absent(&mut response, header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    set_if_absent(&mut response, FRAME_OPTIONS, "SAMEORIGIN");
    set_if_absent(&mut response, header::REFERRER_POLICY, "strict-origin-when-cross-origin");
    set_if_absent(
        &mut response,
        header::CONTENT_SECURITY_POLICY,
        "frame-ancestors 'self'; base-uri 'none'; form-action 'self'",
    );
    if policy.hsts {
        set_if_absent(&mut response, header::STRICT_TRANSPORT_SECURITY, "max-age=31536000");
    }
    response
}

/// Things about the setup that are fine on a laptop and dangerous on a network, said once at start-up.
pub fn setup_warnings(
    database: Option<&aether_core::config_manager::models::DatabaseConfig>,
    development: bool,
    bind_address: &str,
) -> Vec<String> {
    let host = bind_address.rsplit_once(':').map_or(bind_address, |(host, _)| host);
    let on_network = !matches!(host, "127.0.0.1" | "localhost" | "[::1]" | "::1");
    let mut out = Vec::new();
    if let Some(db) = database {
        let local_db = matches!(db.host.as_str(), "127.0.0.1" | "localhost" | "::1" | "[::1]");
        if db.user == "root" && db.password == "root" && (on_network || !local_db) {
            out.push(
                "the database user is `root` with the password `root`, and this server or the database is reachable \
                 from the network: set a real password in SurrealDB and under [database]"
                    .to_string(),
            );
        }
    }
    if development && on_network {
        out.push(
            "is_development_mode is on while the server listens on a network address: session cookies are \
             sent without the Secure flag; turn it off outside your own machine"
                .to_string(),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, routing::get};
    use tower::ServiceExt;

    async fn call(policy: HeaderPolicy) -> Response {
        let app = Router::new()
            .route("/plain", get(|| async { "x" }))
            .route(
                "/strict",
                get(|| async { ([(header::CONTENT_SECURITY_POLICY, "default-src 'none'; sandbox")], "x") }),
            )
            .layer(axum::middleware::from_fn_with_state(policy, add_security_headers));
        app.oneshot(Request::get("/plain").body(Body::empty()).unwrap()).await.unwrap()
    }

    #[test]
    fn a_default_password_is_only_a_problem_on_a_network() {
        let mut db = aether_core::config_manager::models::DatabaseConfig::default();
        db.host = "localhost".into();
        db.user = "root".into();
        db.password = "root".into();
        assert!(setup_warnings(Some(&db), false, "127.0.0.1:7890").is_empty());
        assert_eq!(setup_warnings(Some(&db), false, "0.0.0.0:7890").len(), 1);
        db.password = "a-long-secret".into();
        assert!(setup_warnings(Some(&db), false, "0.0.0.0:7890").is_empty());
        assert_eq!(setup_warnings(None, true, "0.0.0.0:7890").len(), 1, "development mode on a network");
        assert!(setup_warnings(None, true, "localhost:7890").is_empty());
    }

    #[tokio::test]
    async fn every_response_carries_the_headers() {
        let response = call(HeaderPolicy { hsts: true }).await;
        let h = response.headers();
        assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert_eq!(h["x-frame-options"], "SAMEORIGIN");
        assert!(h[header::CONTENT_SECURITY_POLICY].to_str().unwrap().contains("frame-ancestors 'self'"));
        assert_eq!(h[header::STRICT_TRANSPORT_SECURITY], "max-age=31536000");
    }

    #[tokio::test]
    async fn development_does_not_pin_the_browser_to_https() {
        let response = call(HeaderPolicy { hsts: false }).await;
        assert!(response.headers().get(header::STRICT_TRANSPORT_SECURITY).is_none());
    }

    #[tokio::test]
    async fn a_stricter_header_set_by_a_handler_is_kept() {
        let app = Router::new()
            .route(
                "/strict",
                get(|| async { ([(header::CONTENT_SECURITY_POLICY, "default-src 'none'; sandbox")], "x") }),
            )
            .layer(axum::middleware::from_fn_with_state(HeaderPolicy { hsts: false }, add_security_headers));
        let response = app.oneshot(Request::get("/strict").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.headers()[header::CONTENT_SECURITY_POLICY], "default-src 'none'; sandbox");
    }
}
