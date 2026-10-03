//! Logging of every HTTP request.
//!
//! Each request gets an id, which is also the `request_id` of its audit rows
//! and is returned in an `x-request-id` response header, so a line in the
//! server log, a row in the audit trail and a browser's network tab can be
//! matched up. A client-supplied `x-request-id` is never trusted.
//!
//! * `debug`: the request as received (method, URI, client address, headers)
//!   and the response headers.
//! * One line when the response is ready, with status, time taken and size:
//!   `info` for success and for `401` (the normal "please log in" answer),
//!   `warn` for other 4xx, `error` for 5xx. Successful static file requests
//!   under `/web` are `trace`, so the log is not all assets.
//!
//! Secrets are never logged: cookie, authorization and similar headers, and
//! query parameters such as `code` or `token`, are shown as `<redacted>`.
//! Request and response bodies are not logged.

use std::{net::SocketAddr, time::Instant};

use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, HeaderValue, header},
    middleware::Next,
    response::Response,
};

use crate::access::{audit::new_request_id, ip::client_ip};
use crate::state::AppState;

/// Header carrying the request id on the request (for handlers) and response.
pub const REQUEST_ID_HEADER: &str = "x-request-id";

const REDACTED_HEADERS: &[&str] = &[
    "cookie",
    "set-cookie",
    "authorization",
    "proxy-authorization",
    "x-api-key",
    "x-auth-token",
];

const REDACTED_QUERY_KEYS: &[&str] = &[
    "code",
    "state",
    "token",
    "access_token",
    "refresh_token",
    "id_token",
    "password",
    "secret",
    "key",
    "api_key",
    "session",
];

/// `name=value` pairs for logging, with secret values replaced.
pub fn describe_headers(headers: &HeaderMap) -> String {
    headers
        .iter()
        .map(|(name, value)| {
            let shown = if REDACTED_HEADERS.contains(&name.as_str()) {
                "<redacted>".to_string()
            } else {
                value.to_str().unwrap_or("<binary>").to_string()
            };
            format!("{}={shown}", name.as_str())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The query string with the value of every sensitive key replaced.
pub fn redact_query(query: &str) -> String {
    query
        .split('&')
        .map(|pair| match pair.split_once('=') {
            Some((key, _)) if REDACTED_QUERY_KEYS.contains(&key.to_ascii_lowercase().as_str()) => {
                format!("{key}=<redacted>")
            }
            _ => pair.to_string(),
        })
        .collect::<Vec<_>>()
        .join("&")
}

pub async fn log_requests(State(state): State<AppState>, mut request: Request, next: Next) -> Response {
    let started = Instant::now();
    let request_id = new_request_id();
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        // Overwrites anything the client sent: handlers read the id from here.
        request.headers_mut().insert(REQUEST_ID_HEADER, value);
    }

    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let query = request.uri().query().map(redact_query);
    let target = match &query {
        Some(query) => format!("{path}?{query}"),
        None => path.clone(),
    };
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(address)| address.ip());
    let trusted = state
        .config
        .server
        .as_ref()
        .map(|server| server.trusted_proxies.as_slice())
        .unwrap_or_default();
    let ip = client_ip(request.headers(), peer, trusted)
        .map_or_else(|| "unknown".to_string(), |ip| ip.to_string());
    let user_agent = request
        .headers()
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("-")
        .to_string();

    log::debug!(
        "{request_id} --> {method} {target} {:?} from {ip}; headers: {}",
        request.version(),
        describe_headers(request.headers())
    );

    let mut response = next.run(request).await;
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert(REQUEST_ID_HEADER, value);
    }

    let status = response.status();
    let elapsed = started.elapsed();
    let size = response
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .map_or_else(|| "?".to_string(), |bytes| format!("{bytes} B"));

    let quiet_static = path.starts_with("/web") && status.is_success();
    let expected_login_prompt = status == axum::http::StatusCode::UNAUTHORIZED;
    let line = format!(
        "{request_id} {method} {path} -> {status} in {}ms (ip {ip}, {size}, ua \"{user_agent}\")",
        elapsed.as_millis()
    );
    if status.is_server_error() {
        log::error!("{line}");
    } else if status.is_client_error() && !expected_login_prompt {
        log::warn!("{line}");
    } else if quiet_static {
        log::trace!("{line}");
    } else {
        log::info!("{line}");
    }
    log::debug!(
        "{request_id} <-- response headers: {}",
        describe_headers(response.headers())
    );

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_in_headers_are_redacted() {
        let mut headers = HeaderMap::new();
        headers.insert("cookie", HeaderValue::from_static("aether_session=abc123"));
        headers.insert("authorization", HeaderValue::from_static("Bearer xyz"));
        headers.insert("host", HeaderValue::from_static("example.com"));
        let shown = describe_headers(&headers);
        assert!(shown.contains("host=example.com"));
        assert!(shown.contains("cookie=<redacted>"));
        assert!(shown.contains("authorization=<redacted>"));
        assert!(!shown.contains("abc123") && !shown.contains("xyz"));
    }

    #[test]
    fn sensitive_query_values_are_redacted() {
        assert_eq!(
            redact_query("code=SECRET&redirect_uri=http://x&State=also&page=2"),
            "code=<redacted>&redirect_uri=http://x&State=<redacted>&page=2"
        );
        assert_eq!(redact_query("flag"), "flag");
    }
}
