//! `http::request`: a plugin calls an outside web API through the kernel.
//!
//! A plugin has no network of its own, so everything here is the kernel deciding what it may
//! reach:
//!
//! * **Allowlist.** The plugin lists the hosts it needs in `plugin.toml` (`http_hosts`, such as
//!   `"api.stripe.com"` or `"*.example.com"`); any other host is refused. No list, no access.
//! * **Public addresses only.** The host is resolved by the kernel and refused unless every
//!   address is a public one, so an allowed name cannot be pointed at `localhost`, the private
//!   network or a cloud metadata address. The connection is made to the address that was
//!   checked, so the name cannot resolve to something else a moment later.
//! * **No redirects** are followed (a redirect could leave the allowlist); the plugin sees the
//!   `3xx` answer and its `location` header and decides.
//! * `https` only, no credentials in the URL, a request and response of at most 1 MiB, a
//!   timeout of 10 s by default (30 s at most), and no system proxy.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::{Duration, Instant};

use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::Deserialize;
use serde_json::Value as JsonValue;

use super::context::PluginHostContext;
use super::error::HostError;

/// Largest request body and largest response body.
pub const MAX_BODY_BYTES: usize = 1024 * 1024;
const DEFAULT_TIMEOUT_SECS: u64 = 10;
const MAX_TIMEOUT_SECS: u64 = 30;
const MAX_HEADERS: usize = 50;

/// Headers the kernel sets itself or that would let a plugin change how the connection works.
const FORBIDDEN_HEADERS: &[&str] = &[
    "host",
    "content-length",
    "connection",
    "transfer-encoding",
    "upgrade",
    "te",
    "trailer",
    "expect",
    "proxy-authorization",
    "proxy-connection",
    "keep-alive",
];

#[derive(Deserialize)]
struct Request {
    #[serde(default)]
    method: Option<String>,
    url: String,
    #[serde(default)]
    headers: HashMap<String, String>,
    /// Request body as text.
    #[serde(default)]
    body: Option<String>,
    /// Request body as JSON (sets `content-type: application/json` unless given).
    #[serde(default)]
    json: Option<JsonValue>,
    /// Request body as base64.
    #[serde(default)]
    base64: Option<String>,
    #[serde(default)]
    timeout_secs: Option<u64>,
}

/// What a call may do beyond the allowlist. Production always uses [`Rules::STRICT`]; tests
/// relax it to talk to a server on this machine.
#[derive(Clone, Copy)]
struct Rules {
    allow_private_addresses: bool,
    allow_plain_http: bool,
}

impl Rules {
    const STRICT: Self = Self { allow_private_addresses: false, allow_plain_http: false };
}

pub async fn http_request(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("http::request")?;
    let started = Instant::now();
    let answer = execute(&ctx.http_hosts, payload, Rules::STRICT).await;
    let target = payload.get("url").and_then(JsonValue::as_str).and_then(|url| url::Url::parse(url).ok());
    let host = target.as_ref().and_then(|url| url.host_str()).unwrap_or("?");
    match &answer {
        Ok(data) => log::info!(
            "{} http::request {host} -> {} in {}ms",
            ctx.plugin_name,
            data["status"],
            started.elapsed().as_millis()
        ),
        Err(error) => log::warn!("{} http::request {host} refused or failed: {error}", ctx.plugin_name),
    }
    Ok(serde_json::json!({ "ok": true, "data": answer? }))
}

/// Whether `host` is covered by one of the plugin's `http_hosts`. `*.example.com` covers
/// subdomains of `example.com`, not `example.com` itself.
pub fn host_allowed(allowed: &[String], host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    allowed.iter().any(|pattern| {
        let pattern = pattern.trim().trim_end_matches('.').to_ascii_lowercase();
        match pattern.strip_prefix("*.") {
            Some(suffix) => host.len() > suffix.len() + 1 && host.ends_with(&format!(".{suffix}")),
            None => !pattern.is_empty() && pattern == host,
        }
    })
}

/// Whether an address is a public one a plugin may be sent to.
pub fn is_public_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_public_v4(mapped);
            }
            let first = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (first & 0xfe00) == 0xfc00 // unique local, fc00::/7
                || (first & 0xffc0) == 0xfe80 // link local, fe80::/10
                || v6 == Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 0)
                || (first == 0x2001 && v6.segments()[1] == 0x0db8)) // documentation
        }
    }
}

fn is_public_v4(v4: Ipv4Addr) -> bool {
    let [a, b, ..] = v4.octets();
    !(v4.is_loopback()
        || v4.is_private()
        || v4.is_link_local() // 169.254/16, which holds cloud metadata services
        || v4.is_unspecified()
        || v4.is_broadcast()
        || v4.is_multicast()
        || v4.is_documentation()
        || a == 0
        || (a == 100 && (64..=127).contains(&b)) // carrier-grade NAT
        || (a == 192 && b == 0) // protocol assignments
        || (a == 198 && (18..=19).contains(&b)) // benchmarking
        || a >= 240) // reserved
}

fn refuse(message: impl Into<String>) -> HostError {
    HostError::Message(message.into())
}

async fn execute(allowed: &[String], payload: &JsonValue, rules: Rules) -> Result<JsonValue, HostError> {
    let request: Request =
        serde_json::from_value(payload.clone()).map_err(|error| HostError::InvalidPayload(error.to_string()))?;

    let method = match request.method.as_deref().unwrap_or("GET").to_ascii_uppercase().as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "PATCH" => reqwest::Method::PATCH,
        "DELETE" => reqwest::Method::DELETE,
        "HEAD" => reqwest::Method::HEAD,
        other => return Err(HostError::InvalidPayload(format!("method `{other}` is not allowed"))),
    };

    let url = url::Url::parse(&request.url).map_err(|error| HostError::InvalidPayload(format!("url: {error}")))?;
    match url.scheme() {
        "https" => {}
        "http" if rules.allow_plain_http => {}
        _ => return Err(HostError::InvalidPayload("only https:// urls are allowed".into())),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(HostError::InvalidPayload("a url must not carry a username or password".into()));
    }
    let host = url.host_str().ok_or_else(|| HostError::InvalidPayload("the url has no host".into()))?.to_string();
    if !host_allowed(allowed, &host) {
        return Err(refuse(format!(
            "this plugin may not call `{host}`; list it under `http_hosts` in plugin.toml"
        )));
    }
    let port = url.port_or_known_default().ok_or_else(|| HostError::InvalidPayload("the url has no port".into()))?;

    let mut headers = HeaderMap::new();
    if request.headers.len() > MAX_HEADERS {
        return Err(HostError::InvalidPayload(format!("at most {MAX_HEADERS} headers")));
    }
    for (name, value) in &request.headers {
        let lower = name.to_ascii_lowercase();
        if FORBIDDEN_HEADERS.contains(&lower.as_str()) {
            return Err(HostError::InvalidPayload(format!("the `{name}` header is set by the kernel")));
        }
        let name = HeaderName::from_bytes(lower.as_bytes())
            .map_err(|_| HostError::InvalidPayload(format!("`{name}` is not a header name")))?;
        let value = HeaderValue::from_str(value)
            .map_err(|_| HostError::InvalidPayload(format!("the value of `{name}` is not a valid header value")))?;
        headers.insert(name, value);
    }

    let body_choices = [request.body.is_some(), request.json.is_some(), request.base64.is_some()];
    if body_choices.iter().filter(|set| **set).count() > 1 {
        return Err(HostError::InvalidPayload("give only one of `body`, `json` and `base64`".into()));
    }
    let body: Option<Vec<u8>> = if let Some(text) = request.body {
        Some(text.into_bytes())
    } else if let Some(json) = request.json {
        headers
            .entry(reqwest::header::CONTENT_TYPE)
            .or_insert(HeaderValue::from_static("application/json"));
        Some(serde_json::to_vec(&json).map_err(|error| HostError::InvalidPayload(error.to_string()))?)
    } else if let Some(encoded) = request.base64 {
        Some(
            STANDARD
                .decode(encoded.as_bytes())
                .map_err(|error| HostError::InvalidPayload(format!("base64: {error}")))?,
        )
    } else {
        None
    };
    if body.as_ref().is_some_and(|bytes| bytes.len() > MAX_BODY_BYTES) {
        return Err(HostError::InvalidPayload(format!("a request body is at most {MAX_BODY_BYTES} bytes")));
    }

    // Resolve here, check every address, and connect to one of those, not to the name.
    let addresses: Vec<SocketAddr> = tokio::time::timeout(Duration::from_secs(5), tokio::net::lookup_host((host.as_str(), port)))
        .await
        .map_err(|_| refuse(format!("`{host}` did not resolve in time")))?
        .map_err(|_| refuse(format!("`{host}` could not be resolved")))?
        .collect();
    if addresses.is_empty() {
        return Err(refuse(format!("`{host}` has no address")));
    }
    if !rules.allow_private_addresses && addresses.iter().any(|address| !is_public_address(address.ip())) {
        return Err(refuse(format!("`{host}` points at a private or reserved address")));
    }

    let timeout = Duration::from_secs(request.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS).clamp(1, MAX_TIMEOUT_SECS));
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .timeout(timeout)
        .user_agent("Aether-Plugin")
        .resolve_to_addrs(&host, &addresses)
        .build()
        .map_err(|error| refuse(format!("could not prepare the request: {error}")))?;

    let mut builder = client.request(method, url).headers(headers);
    if let Some(bytes) = body {
        builder = builder.body(bytes);
    }
    let mut response = builder.send().await.map_err(|error| {
        if error.is_timeout() {
            refuse(format!("`{host}` did not answer within {}s", timeout.as_secs()))
        } else {
            refuse(format!("the request to `{host}` failed ({})", describe(&error)))
        }
    })?;

    let status = response.status();
    let mut response_headers = serde_json::Map::new();
    for (name, value) in response.headers() {
        if let Ok(value) = value.to_str() {
            // A repeated header keeps its last value; plugins rarely need more.
            response_headers.insert(name.as_str().to_string(), JsonValue::String(value.to_string()));
        }
    }
    let mut bytes: Vec<u8> = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| refuse(format!("the answer from `{host}` was cut off ({})", describe(&error))))?
    {
        if bytes.len() + chunk.len() > MAX_BODY_BYTES {
            return Err(refuse(format!("the answer from `{host}` is larger than {MAX_BODY_BYTES} bytes")));
        }
        bytes.extend_from_slice(&chunk);
    }

    let mut data = serde_json::json!({
        "status": status.as_u16(),
        "ok": status.is_success(),
        "headers": response_headers,
    });
    match String::from_utf8(bytes) {
        Ok(text) => data["body"] = JsonValue::String(text),
        Err(error) => data["body_base64"] = JsonValue::String(STANDARD.encode(error.into_bytes())),
    }
    Ok(data)
}

/// A short reason for a failed request, without URLs or addresses from the error chain.
fn describe(error: &reqwest::Error) -> &'static str {
    if error.is_connect() {
        "could not connect"
    } else if error.is_timeout() {
        "timed out"
    } else if error.is_body() || error.is_decode() {
        "bad response body"
    } else {
        "network error"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const LOCAL: Rules = Rules { allow_private_addresses: true, allow_plain_http: true };

    fn hosts(list: &[&str]) -> Vec<String> {
        list.iter().map(|host| (*host).to_string()).collect()
    }

    #[test]
    fn host_patterns() {
        let list = hosts(&["api.example.com", "*.cdn.example.org"]);
        assert!(host_allowed(&list, "api.example.com"));
        assert!(host_allowed(&list, "API.Example.COM."));
        assert!(host_allowed(&list, "a.cdn.example.org"));
        assert!(host_allowed(&list, "a.b.cdn.example.org"));
        assert!(!host_allowed(&list, "cdn.example.org"), "a wildcard does not cover the apex");
        assert!(!host_allowed(&list, "evilcdn.example.org"));
        assert!(!host_allowed(&list, "api.example.com.evil.net"));
        assert!(!host_allowed(&list, "example.com"));
        assert!(!host_allowed(&[], "api.example.com"), "no list, no access");
        assert!(!host_allowed(&hosts(&[""]), ""));
    }

    #[test]
    fn only_public_addresses_pass() {
        for blocked in [
            "127.0.0.1", "10.1.2.3", "172.16.0.1", "172.31.255.255", "192.168.1.1", "169.254.169.254", "0.0.0.0",
            "100.64.0.1", "224.0.0.1", "255.255.255.255", "192.0.2.1", "198.18.0.1", "240.0.0.1", "::1", "::",
            "fe80::1", "fc00::1", "fd12:3456::1", "ff02::1", "::ffff:127.0.0.1", "::ffff:10.0.0.1", "2001:db8::1",
        ] {
            let address: IpAddr = blocked.parse().unwrap();
            assert!(!is_public_address(address), "{blocked} must be refused");
        }
        for open in ["8.8.8.8", "1.1.1.1", "93.184.216.34", "2606:4700:4700::1111", "172.32.0.1", "::ffff:8.8.8.8"] {
            let address: IpAddr = open.parse().unwrap();
            assert!(is_public_address(address), "{open} must pass");
        }
    }

    #[tokio::test]
    async fn refusals_come_before_any_connection() {
        let list = hosts(&["api.example.com", "localhost", "127.0.0.1"]);
        let strict = Rules::STRICT;
        let call = |value: JsonValue| {
            let list = list.clone();
            async move { execute(&list, &value, strict).await }
        };

        let not_listed = call(serde_json::json!({ "url": "https://evil.example.net/" })).await;
        assert!(matches!(&not_listed, Err(HostError::Message(m)) if m.contains("http_hosts")), "{not_listed:?}");
        for bad in [
            serde_json::json!({ "url": "http://api.example.com/" }),
            serde_json::json!({ "url": "ftp://api.example.com/" }),
            serde_json::json!({ "url": "https://user:pw@api.example.com/" }),
            serde_json::json!({ "url": "not a url" }),
            serde_json::json!({ "url": "https://api.example.com/", "method": "TRACE" }),
            serde_json::json!({ "url": "https://api.example.com/", "headers": { "Host": "x" } }),
            serde_json::json!({ "url": "https://api.example.com/", "body": "a", "json": 1 }),
            serde_json::json!({ "url": "https://api.example.com/", "body": "x".repeat(MAX_BODY_BYTES + 1) }),
        ] {
            let result = call(bad.clone()).await;
            assert!(matches!(result, Err(HostError::InvalidPayload(_))), "{bad}: {result:?}");
        }
        // Allowed by name but pointing at this machine.
        for target in ["https://localhost/", "https://127.0.0.1/"] {
            let result = call(serde_json::json!({ "url": target })).await;
            assert!(matches!(&result, Err(HostError::Message(m)) if m.contains("private")), "{target}: {result:?}");
        }
    }

    /// A one-shot HTTP server on this machine that answers with `reply` and reports the request.
    async fn serve_once(reply: Vec<u8>) -> (u16, tokio::sync::oneshot::Receiver<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut seen = Vec::new();
            let mut buffer = [0u8; 4096];
            loop {
                let read = socket.read(&mut buffer).await.unwrap();
                seen.extend_from_slice(&buffer[..read]);
                let text = String::from_utf8_lossy(&seen).to_string();
                if let Some(head_end) = text.find("\r\n\r\n") {
                    let length = text
                        .lines()
                        .find_map(|line| line.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                        .unwrap_or(0);
                    if seen.len() >= head_end + 4 + length {
                        break;
                    }
                }
                if read == 0 {
                    break;
                }
            }
            let _ = tx.send(String::from_utf8_lossy(&seen).to_string());
            socket.write_all(&reply).await.unwrap();
            socket.shutdown().await.ok();
        });
        (port, rx)
    }

    #[tokio::test]
    async fn sends_the_request_and_returns_status_headers_and_body() {
        let reply = b"HTTP/1.1 201 Created\r\nx-thing: yes\r\ncontent-length: 5\r\n\r\nhello".to_vec();
        let reply = String::from_utf8(reply).unwrap().replace("\\\\r\\\\n", "\r\n").into_bytes();
        let (port, seen) = serve_once(reply).await;
        let list = hosts(&["localhost"]);
        let answer = execute(
            &list,
            &serde_json::json!({
                "method": "post",
                "url": format!("http://localhost:{port}/path?x=1"),
                "headers": { "X-Key": "abc" },
                "json": { "a": 1 },
            }),
            LOCAL,
        )
        .await
        .unwrap();
        assert_eq!(answer["status"], 201);
        assert_eq!(answer["ok"], true);
        assert_eq!(answer["body"], "hello");
        assert_eq!(answer["headers"]["x-thing"], "yes");

        let request = seen.await.unwrap().to_ascii_lowercase();
        assert!(request.starts_with("post /path?x=1 http/1.1"), "{request}");
        assert!(request.contains("x-key: abc"));
        assert!(request.contains("content-type: application/json"));
        assert!(request.ends_with("{\"a\":1}"), "{request}");
    }

    #[tokio::test]
    async fn redirects_are_not_followed() {
        let reply = "HTTP/1.1 302 Found\r\nlocation: http://169.254.169.254/\r\ncontent-length: 0\r\n\r\n".replace("\\\\r\\\\n", "\r\n");
        let (port, _seen) = serve_once(reply.into_bytes()).await;
        let answer = execute(&hosts(&["localhost"]), &serde_json::json!({ "url": format!("http://localhost:{port}/") }), LOCAL)
            .await
            .unwrap();
        assert_eq!(answer["status"], 302);
        assert_eq!(answer["headers"]["location"], "http://169.254.169.254/");
    }

    #[tokio::test]
    async fn an_oversized_answer_is_refused() {
        let mut reply = format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n", MAX_BODY_BYTES + 10).into_bytes();
        reply.extend(std::iter::repeat_n(b'a', MAX_BODY_BYTES + 10));
        let (port, _seen) = serve_once(reply).await;
        let result = execute(&hosts(&["localhost"]), &serde_json::json!({ "url": format!("http://localhost:{port}/") }), LOCAL).await;
        assert!(matches!(&result, Err(HostError::Message(m)) if m.contains("larger")), "{result:?}");
    }
}
