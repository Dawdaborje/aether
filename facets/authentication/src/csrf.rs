//! Double-submit CSRF tokens and Origin / Sec-Fetch-Site checks for cookie auth.

use std::net::IpAddr;

use aether_core::state::AppState;
use axum::{
    Json,
    extract::{ConnectInfo, Request, State},
    http::{
        HeaderMap, HeaderValue, Method, StatusCode,
        header::{self, SET_COOKIE},
    },
    middleware::Next,
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde_json::json;
use time::Duration;

pub const CSRF_COOKIE: &str = "aether_csrf";
pub const CSRF_HEADER: &str = "x-csrf-token";
pub const OAUTH_STATE_COOKIE: &str = "aether_oauth_state";

const CSRF_MAX_AGE_DAYS: i64 = 1;
const OAUTH_STATE_MAX_AGE_MINUTES: i64 = 10;

/// Cryptographically random hex token (32 bytes → 64 hex chars).
pub fn new_token() -> String {
    use rand::RngExt;
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Constant-time equality for equal-length strings; unequal lengths are never equal.
pub fn tokens_equal(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub fn read_csrf_token(jar: &CookieJar) -> Option<String> {
    jar.get(CSRF_COOKIE)
        .map(|c| c.value().to_string())
        .filter(|v| !v.is_empty())
}

pub fn build_csrf_cookie(token: String, secure: bool) -> Cookie<'static> {
    Cookie::build((CSRF_COOKIE, token))
        .http_only(false)
        .secure(secure)
        .path("/")
        .same_site(SameSite::Lax)
        .max_age(Duration::days(CSRF_MAX_AGE_DAYS))
        .build()
}

/// Issue a fresh CSRF cookie (login / logout / org switch / OAuth success).
pub fn rotate_csrf_cookie(jar: CookieJar, secure: bool) -> CookieJar {
    jar.add(build_csrf_cookie(new_token(), secure))
}

/// Ensure a CSRF cookie exists; leave an existing one alone.
pub fn ensure_csrf_cookie(jar: CookieJar, secure: bool) -> CookieJar {
    if read_csrf_token(&jar).is_some() {
        jar
    } else {
        jar.add(build_csrf_cookie(new_token(), secure))
    }
}

pub fn read_oauth_state(jar: &CookieJar) -> Option<String> {
    jar.get(OAUTH_STATE_COOKIE)
        .map(|c| c.value().to_string())
        .filter(|v| !v.is_empty())
}

pub fn set_oauth_state_cookie(jar: CookieJar, state: &str, secure: bool) -> CookieJar {
    let cookie = Cookie::build((OAUTH_STATE_COOKIE, state.to_string()))
        .http_only(true)
        .secure(secure)
        .path("/")
        .same_site(SameSite::Lax)
        .max_age(Duration::minutes(OAUTH_STATE_MAX_AGE_MINUTES))
        .build();
    jar.add(cookie)
}

pub fn clear_oauth_state_cookie(jar: CookieJar, secure: bool) -> CookieJar {
    let cookie = Cookie::build((OAUTH_STATE_COOKIE, ""))
        .http_only(true)
        .secure(secure)
        .path("/")
        .same_site(SameSite::Lax)
        .max_age(Duration::seconds(0))
        .build();
    jar.add(cookie)
}

pub fn cookies_secure(state: &AppState) -> bool {
    !state
        .config
        .configuration
        .as_ref()
        .is_some_and(|core| core.is_development_mode)
}

fn is_unsafe_method(method: &Method) -> bool {
    matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

fn is_api_path(path: &str) -> bool {
    path == "/api" || path.starts_with("/api/")
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

/// Origin of the page that issued the request (`https://host:port`), or `None`.
pub fn origin_from_referer(referer: &str) -> Option<String> {
    let url = url::Url::parse(referer).ok()?;
    let origin = url.origin().ascii_serialization();
    if origin == "null" {
        None
    } else {
        Some(origin)
    }
}

fn default_scheme(host: &str) -> &'static str {
    let host = host.split(':').next().unwrap_or(host);
    if host == "localhost" || host == "127.0.0.1" || host == "[::1]" || host == "::1" {
        "http"
    } else {
        "https"
    }
}

/// Scheme+host(+port) implied by this request's Host (and forwarded headers
/// only when the TCP peer is a trusted proxy).
pub fn request_origin(
    headers: &HeaderMap,
    peer: Option<IpAddr>,
    trusted_proxies: &[IpAddr],
) -> Option<String> {
    let from_proxy = peer.is_some_and(|p| trusted_proxies.contains(&p));
    let host = if from_proxy {
        header_str(headers, "x-forwarded-host").or_else(|| header_str(headers, header::HOST.as_str()))
    } else {
        header_str(headers, header::HOST.as_str())
    }?;
    let host = host.split(',').next()?.trim();
    if host.is_empty() {
        return None;
    }
    let scheme = if from_proxy {
        header_str(headers, "x-forwarded-proto")
            .map(|s| s.split(',').next().unwrap_or(s).trim())
            .filter(|s| *s == "http" || *s == "https")
            .unwrap_or_else(|| default_scheme(host))
    } else {
        default_scheme(host)
    };
    Some(format!("{scheme}://{host}"))
}

/// Whether `candidate` is the request origin or listed in `trusted`.
pub fn origin_allowed(candidate: &str, request_origin: Option<&str>, trusted: &[String]) -> bool {
    if request_origin.is_some_and(|o| o == candidate) {
        return true;
    }
    trusted.iter().any(|o| o == candidate)
}

/// Origin / Sec-Fetch-Site gate for unsafe methods.
pub fn check_site(
    headers: &HeaderMap,
    peer: Option<IpAddr>,
    trusted_proxies: &[IpAddr],
    trusted_origins: &[String],
) -> Result<(), ()> {
    let sec_fetch_site = header_str(headers, "sec-fetch-site").map(str::to_ascii_lowercase);
    if sec_fetch_site.as_deref() == Some("cross-site") {
        return Err(());
    }

    let req_origin = request_origin(headers, peer, trusted_proxies);

    if let Some(origin) = header_str(headers, header::ORIGIN.as_str()) {
        if origin == "null" || origin.is_empty() {
            return Err(());
        }
        return if origin_allowed(origin, req_origin.as_deref(), trusted_origins) {
            Ok(())
        } else {
            Err(())
        };
    }

    if let Some(referer) = header_str(headers, header::REFERER.as_str()) {
        let Some(ref_origin) = origin_from_referer(referer) else {
            return Err(());
        };
        return if origin_allowed(&ref_origin, req_origin.as_deref(), trusted_origins) {
            Ok(())
        } else {
            Err(())
        };
    }

    match sec_fetch_site.as_deref() {
        Some("same-origin") | Some("same-site") => Ok(()),
        _ => Err(()),
    }
}

fn csrf_forbidden() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({ "error": "csrf" })),
    )
        .into_response()
}

fn append_cookie(response: &mut Response, cookie: Cookie<'static>) {
    if let Ok(value) = HeaderValue::from_str(&cookie.to_string()) {
        response.headers_mut().append(SET_COOKIE, value);
    }
}

/// Reject unsafe `/api` calls without a matching double-submit token and a
/// same-site Origin signal; bootstrap a CSRF cookie on `/api` responses when missing.
pub async fn csrf_protect(
    State(state): State<AppState>,
    jar: CookieJar,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();
    let method = request.method().clone();
    let api = is_api_path(&path);

    if api && is_unsafe_method(&method) {
        let peer = request
            .extensions()
            .get::<ConnectInfo<std::net::SocketAddr>>()
            .map(|ConnectInfo(address)| address.ip());
        let trusted_proxies = state
            .config
            .server
            .as_ref()
            .map(|s| s.trusted_proxies.as_slice())
            .unwrap_or(&[]);
        let trusted_origins = state
            .config
            .server
            .as_ref()
            .map(|s| s.csrf_trusted_origins.as_slice())
            .unwrap_or(&[]);

        if check_site(request.headers(), peer, trusted_proxies, trusted_origins).is_err() {
            let mut response = csrf_forbidden();
            if read_csrf_token(&jar).is_none() {
                append_cookie(
                    &mut response,
                    build_csrf_cookie(new_token(), cookies_secure(&state)),
                );
            }
            return response;
        }

        let cookie_token = read_csrf_token(&jar);
        let header_token = header_str(request.headers(), CSRF_HEADER).map(str::to_string);
        match (cookie_token.as_deref(), header_token.as_deref()) {
            (Some(c), Some(h)) if tokens_equal(c, h) => {}
            _ => {
                let mut response = csrf_forbidden();
                if cookie_token.is_none() {
                    append_cookie(
                        &mut response,
                        build_csrf_cookie(new_token(), cookies_secure(&state)),
                    );
                }
                return response;
            }
        }
    }

    let had_csrf = read_csrf_token(&jar).is_some();
    let mut response = next.run(request).await;

    if api && !had_csrf {
        // Handlers that rotate CSRF already set the cookie; skip if present.
        let already_set = response
            .headers()
            .get_all(SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .any(|v| v.starts_with(&format!("{CSRF_COOKIE}=")));
        if !already_set {
            append_cookie(
                &mut response,
                build_csrf_cookie(new_token(), cookies_secure(&state)),
            );
        }
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                HeaderValue::from_str(value).unwrap(),
            );
        }
        map
    }

    #[test]
    fn tokens_equal_rejects_length_mismatch_and_differs() {
        assert!(tokens_equal("abcd", "abcd"));
        assert!(!tokens_equal("abcd", "abce"));
        assert!(!tokens_equal("abc", "abcd"));
        assert!(!tokens_equal("", "a"));
    }

    #[test]
    fn new_token_is_long_hex() {
        let token = new_token();
        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(token, new_token());
    }

    #[test]
    fn origin_from_referer_strips_path() {
        assert_eq!(
            origin_from_referer("https://app.example.com/web/login?x=1"),
            Some("https://app.example.com".into())
        );
        assert_eq!(
            origin_from_referer("http://localhost:5173/"),
            Some("http://localhost:5173".into())
        );
    }

    #[test]
    fn request_origin_uses_host_and_localhost_http() {
        let h = headers(&[("host", "localhost:7890")]);
        assert_eq!(
            request_origin(&h, None, &[]),
            Some("http://localhost:7890".into())
        );
        let h = headers(&[("host", "app.example.com")]);
        assert_eq!(
            request_origin(&h, None, &[]),
            Some("https://app.example.com".into())
        );
    }

    #[test]
    fn request_origin_honours_forwarded_headers_from_trusted_proxy() {
        let proxy: IpAddr = "10.0.0.1".parse().unwrap();
        let h = headers(&[
            ("host", "internal:7890"),
            ("x-forwarded-host", "app.example.com"),
            ("x-forwarded-proto", "https"),
        ]);
        assert_eq!(
            request_origin(&h, Some(proxy), &[proxy]),
            Some("https://app.example.com".into())
        );
        // Untrusted peer: ignore forwarded headers.
        assert_eq!(
            request_origin(&h, Some("203.0.113.9".parse().unwrap()), &[proxy]),
            Some("https://internal:7890".into())
        );
    }

    #[test]
    fn check_site_rejects_cross_site_and_bad_origin() {
        let h = headers(&[
            ("sec-fetch-site", "cross-site"),
            ("origin", "https://evil.example"),
            ("host", "app.example.com"),
        ]);
        assert!(check_site(&h, None, &[], &[]).is_err());

        let h = headers(&[
            ("origin", "https://evil.example"),
            ("host", "app.example.com"),
        ]);
        assert!(check_site(&h, None, &[], &[]).is_err());

        let h = headers(&[
            ("origin", "https://app.example.com"),
            ("host", "app.example.com"),
        ]);
        assert!(check_site(&h, None, &[], &[]).is_ok());

        let trusted = vec!["http://localhost:5173".into()];
        let h = headers(&[
            ("origin", "http://localhost:5173"),
            ("host", "127.0.0.1:7890"),
        ]);
        assert!(check_site(&h, None, &[], &trusted).is_ok());
    }

    #[test]
    fn check_site_allows_same_origin_sec_fetch_without_origin() {
        let h = headers(&[
            ("sec-fetch-site", "same-origin"),
            ("host", "app.example.com"),
        ]);
        assert!(check_site(&h, None, &[], &[]).is_ok());
    }

    #[test]
    fn check_site_uses_referer_when_origin_absent() {
        let h = headers(&[
            ("referer", "https://app.example.com/web/"),
            ("host", "app.example.com"),
        ]);
        assert!(check_site(&h, None, &[], &[]).is_ok());
    }

    #[test]
    fn check_site_rejects_when_no_site_signal() {
        let h = headers(&[("host", "app.example.com")]);
        assert!(check_site(&h, None, &[], &[]).is_err());
    }

    #[test]
    fn oauth_state_cookie_round_trip() {
        let jar = CookieJar::new();
        assert!(read_oauth_state(&jar).is_none());
        let jar = set_oauth_state_cookie(jar, "state-abc", false);
        assert_eq!(read_oauth_state(&jar).as_deref(), Some("state-abc"));
        let jar = clear_oauth_state_cookie(jar, false);
        assert!(read_oauth_state(&jar).is_none());
    }

    #[test]
    fn csrf_cookie_rotate_changes_value() {
        let jar = rotate_csrf_cookie(CookieJar::new(), false);
        let first = read_csrf_token(&jar).expect("token");
        let jar = rotate_csrf_cookie(jar, false);
        let second = read_csrf_token(&jar).expect("token");
        assert_ne!(first, second);
        assert!(tokens_equal(&first, &first));
        assert!(!tokens_equal(&first, &second));
    }
}
