//! Inbound hooks: addresses an outside system calls to tell a plugin something (see
//! `docs/architecture/api_and_webhooks.md`).
//!
//! ```text
//! POST /api/hooks/<organization>/<plugin>/<name>      the other system calls this
//! GET  /api/ui/inbound/<plugin>                       an administrator: the plugin's hooks and their state
//! POST /api/ui/inbound/<plugin>/<name>/secret         make or rotate the hook's secret (shown once)
//! PATCH /api/ui/inbound/<plugin>/<name> { enabled }   switch it off or on
//! DELETE /api/ui/inbound/<plugin>/<name>              remove the secret and the hook's counters
//! ```
//!
//! A plugin declares a hook in `plugin.toml` (`[[inbound]]`, see [`PluginInboundDef`]); it works only
//! after an administrator has made its secret. A call is checked **before any plugin code runs**:
//!
//! * `verify = "aether"`: `X-Aether-Signature: t=<unix>,v1=<hex HMAC-SHA256 of "<t>.<body>">`, as our own
//!   webhooks send; the time must be within `tolerance_secs` (300) of now, so a captured call cannot be
//!   replayed later;
//! * `verify = "hmac-sha256"`: the named header holds the hex HMAC-SHA256 of the raw body, optionally
//!   after `sha256=` (the form many services use);
//! * `verify = "token"`: the named header holds the secret itself (compared in constant time).
//!
//! A hook that does not exist, is not installed, has no secret or is switched off answers `404`, whoever
//! asks, so the address says nothing about what is behind it. A call that fails the check answers `401`
//! and is counted. Both the body (at most 1 MiB) and the rate (the anonymous per-address budget) are
//! limited. The route needs no session and has no CSRF check: the signature is the credential, and
//! cookies are never looked at.
//!
//! A verified call runs the plugin's function as the kernel (`hook:<plugin>.<name>`), with the plugin's
//! own capabilities, and answers what it returns. The call is **synchronous** and the sender may retry, so
//! the function should be safe to repeat.

use std::net::SocketAddr;

use axum::{
    Extension, Json, Router,
    body::Body,
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Map, Value, json};
use sha2::Sha256;

use crate::plugin_manager::models::plugin_def::PluginInboundDef;
use crate::state::AppState;
use crate::webhooks_api::{Admin, admin, failure, internal};

/// Largest body accepted.
pub const MAX_BODY_BYTES: usize = 1024 * 1024;
const DEFAULT_TOLERANCE_SECS: u64 = 300;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/hooks/{org}/{plugin}/{name}", post(receive))
        .route("/api/ui/inbound/{plugin}", get(describe))
        .route("/api/ui/inbound/{plugin}/{name}/secret", post(make_secret))
        .route(
            "/api/ui/inbound/{plugin}/{name}",
            axum::routing::patch(switch).delete(remove),
        )
}

/// Why a call was not let in. Said to the log and the administrator, never to the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reject {
    /// The header is not there.
    Missing,
    /// The header is there but is not in the form the hook expects.
    Malformed,
    /// The signature or token does not match any secret.
    Mismatch,
    /// The signed time is too far from now.
    Stale,
}

impl Reject {
    pub fn describe(self) -> &'static str {
        match self {
            Self::Missing => "the signature header was missing",
            Self::Malformed => "the signature header was malformed",
            Self::Mismatch => "the signature did not match",
            Self::Stale => "the signed time was too far from now",
        }
    }
}

/// Compare two byte strings without stopping at the first difference.
pub(crate) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |diff, (x, y)| diff | (x ^ y)) == 0
}

fn hmac_hex(secret: &str, body: &[u8]) -> String {
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return String::new();
    };
    mac.update(body);
    mac.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

/// Check a call against the hook's definition and the secrets in force. `header` is the value of the
/// hook's header; `now` is unix seconds.
pub fn verify(
    def: &PluginInboundDef,
    secrets: &[String],
    header: Option<&str>,
    body: &[u8],
    now: i64,
) -> Result<(), Reject> {
    let value = header.map(str::trim).filter(|v| !v.is_empty()).ok_or(Reject::Missing)?;
    match def.verify.as_str() {
        "token" => secrets
            .iter()
            .any(|secret| constant_time_eq(value.as_bytes(), secret.as_bytes()))
            .then_some(())
            .ok_or(Reject::Mismatch),
        "hmac-sha256" => {
            let given = value
                .strip_prefix("sha256=")
                .or_else(|| value.strip_prefix("SHA256="))
                .unwrap_or(value)
                .to_ascii_lowercase();
            if given.len() != 64 || !given.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(Reject::Malformed);
            }
            secrets
                .iter()
                .any(|secret| constant_time_eq(given.as_bytes(), hmac_hex(secret, body).as_bytes()))
                .then_some(())
                .ok_or(Reject::Mismatch)
        }
        _ => {
            // t=<unix>,v1=<hex>[,v1=<hex>]
            let mut time = None;
            let mut signatures = Vec::new();
            for part in value.split(',') {
                match part.trim().split_once('=') {
                    Some(("t", t)) => time = t.parse::<i64>().ok(),
                    Some(("v1", sig)) => signatures.push(sig.to_ascii_lowercase()),
                    _ => {}
                }
            }
            let (Some(time), false) = (time, signatures.is_empty()) else {
                return Err(Reject::Malformed);
            };
            let tolerance = i64::try_from(def.tolerance_secs.unwrap_or(DEFAULT_TOLERANCE_SECS)).unwrap_or(i64::MAX);
            // The signature is checked first, so a stranger cannot tell a stale time from a wrong secret.
            let genuine = secrets.iter().any(|secret| {
                let expected = crate::webhooks::sign(secret, time, body);
                signatures.iter().any(|sig| constant_time_eq(sig.as_bytes(), expected.as_bytes()))
            });
            if !genuine {
                return Err(Reject::Mismatch);
            }
            if (now - time).abs() > tolerance {
                return Err(Reject::Stale);
            }
            Ok(())
        }
    }
}

/// The headers the plugin function sees: not the credentials, not the signature.
fn visible_headers(headers: &HeaderMap, signature_header: &str) -> Map<String, Value> {
    let mut out = Map::new();
    for (name, value) in headers {
        let name = name.as_str().to_ascii_lowercase();
        if name == signature_header
            || ["authorization", "cookie", "set-cookie", "proxy-authorization"].contains(&name.as_str())
            || ["signature", "secret", "token", "api-key", "apikey"].iter().any(|bad| name.contains(bad))
        {
            continue;
        }
        if let Ok(value) = value.to_str()
            && out.len() < 40
        {
            out.insert(name, Value::String(value.chars().take(512).collect()));
        }
    }
    out
}

fn not_found() -> Response {
    failure(StatusCode::NOT_FOUND, "not found")
}

/// `POST /api/hooks/<organization>/<plugin>/<name>`.
async fn receive(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path((org, plugin, name)): Path<(String, String, String)>,
    Query(query): Query<std::collections::HashMap<String, String>>,
    headers: HeaderMap,
    request: Request<Body>,
) -> Response {
    let peer_ip = peer.map(|Extension(ConnectInfo(address))| address.ip());
    let trusted = state
        .config
        .server
        .as_ref()
        .map(|server| server.trusted_proxies.as_slice())
        .unwrap_or_default();
    let ip = crate::access::ip::client_ip(&headers, peer_ip, trusted);
    if !state.limits.request_limiter.allow(ip) {
        return crate::access::identity::with_retry_after(failure(StatusCode::TOO_MANY_REQUESTS, "too many requests"));
    }
    // The organization must exist; nothing else is learned from the answer.
    match crate::access::org_databases::list_cached(&state).await {
        Ok(names) if names.iter().any(|candidate| *candidate == org) => {}
        Ok(_) => return not_found(),
        Err(error) => return internal(error),
    }
    let Ok(db) = state.org(&org).await else {
        return not_found();
    };
    let Ok(core) = state.core().await else {
        return failure(StatusCode::INTERNAL_SERVER_ERROR, "database error");
    };
    let installed = match crate::plugin_manager::installed::list_cached(&state.cache, &org, &db).await {
        Ok(rows) => rows,
        Err(error) => return internal(error),
    };
    let Some(row) = installed.into_iter().find(|row| row.plugin_name == plugin && row.is_enabled) else {
        return not_found();
    };
    let Ok(loaded) = state.plugin_runtime.ensure_loaded(&core, &plugin, &row.version).await else {
        return not_found();
    };
    let Some(def) = loaded.manifest.inbound.iter().find(|hook| hook.name == name).cloned() else {
        return not_found();
    };
    let mut response = match db
        .query("SELECT * FROM inbound_hooks WHERE plugin = $plugin AND name = $name LIMIT 1;")
        .bind(("plugin", plugin.clone()))
        .bind(("name", name.clone()))
        .await
        .and_then(|r| r.check())
    {
        Ok(response) => response,
        Err(error) => return internal(error),
    };
    let rows: Vec<Value> = response.take(0).unwrap_or_default();
    let Some(hook) = rows.first().filter(|hook| hook.get("enabled").and_then(Value::as_bool) == Some(true)) else {
        return not_found();
    };

    // The body, up to its limit, without reading more than that.
    let body = match axum::body::to_bytes(request.into_body(), MAX_BODY_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => return failure(StatusCode::PAYLOAD_TOO_LARGE, "the body is larger than 1 MiB"),
    };

    let Ok(secrets_box) = state.secrets() else {
        return failure(StatusCode::INTERNAL_SERVER_ERROR, "the encryption key is unavailable");
    };
    let mut secrets = Vec::new();
    if let Some(current) = hook.get("secret").and_then(Value::as_str).and_then(|s| secrets_box.decrypt(s).ok()) {
        secrets.push(current);
    }
    let rotating = hook
        .get("secret_prev_until")
        .and_then(Value::as_str)
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        .is_some_and(|until| until > chrono::Utc::now());
    if rotating
        && let Some(previous) = hook.get("secret_prev").and_then(Value::as_str).and_then(|s| secrets_box.decrypt(s).ok())
    {
        secrets.push(previous);
    }
    let header_name = def.header_name();
    let given = headers.get(&header_name).and_then(|value| value.to_str().ok());
    if let Err(reason) = verify(&def, &secrets, given, &body, chrono::Utc::now().timestamp()) {
        log::warn!("{org}: inbound hook {plugin}.{name} refused a call from {ip:?}: {}", reason.describe());
        let _ = db
            .query(
                "UPDATE inbound_hooks SET rejected += 1, last_rejected_at = time::now(), last_rejected_reason = $why \
                 WHERE plugin = $plugin AND name = $name;",
            )
            .bind(("why", reason.describe().to_string()))
            .bind(("plugin", plugin.clone()))
            .bind(("name", name.clone()))
            .await;
        return failure(StatusCode::UNAUTHORIZED, "invalid signature");
    }

    let raw = String::from_utf8_lossy(&body).into_owned();
    let content_type = headers.get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let parsed = if content_type.contains("json") || raw.trim_start().starts_with(['{', '[']) {
        serde_json::from_slice::<Value>(&body).unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    let payload = json!({
        "hook": name,
        "body": parsed,
        "raw": raw,
        "content_type": content_type,
        "headers": visible_headers(&headers, &header_name),
        "query": query,
        "received_at": chrono::Utc::now().to_rfc3339(),
        "remote_ip": state.limits.ip_policy.apply(ip),
    });
    let _ = db
        .query("UPDATE inbound_hooks SET received += 1, last_received_at = time::now() WHERE plugin = $plugin AND name = $name;")
        .bind(("plugin", plugin.clone()))
        .bind(("name", name.clone()))
        .await;
    let request_id = headers
        .get(crate::request_log::REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .unwrap_or_else(crate::access::audit::new_request_id);
    let agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(crate::access::audit::clip_user_agent)
        .unwrap_or_default();
    match crate::plugin_manager::api::run_system_call_from(
        &state,
        &org,
        &format!("hook:{plugin}.{name}"),
        state.limits.ip_policy.apply(ip),
        &agent,
        &plugin,
        &def.function,
        payload,
        &request_id,
    )
    .await
    {
        Ok(Value::Object(answer)) => Json(Value::Object(answer)).into_response(),
        Ok(_) => Json(json!({ "ok": true })).into_response(),
        Err(error) => {
            log::warn!("{org}: inbound hook {plugin}.{name}: {}", error.message);
            // Never the plugin's own words to a stranger; a permanent failure tells the sender not to retry.
            if error.permanent {
                failure(StatusCode::UNPROCESSABLE_ENTITY, "the hook cannot take this")
            } else {
                failure(StatusCode::INTERNAL_SERVER_ERROR, "the hook failed")
            }
        }
    }
}

/// The plugin as loaded in the administrator's organization, or a response saying why not.
async fn declared(
    state: &AppState,
    admin: &Admin,
    plugin: &str,
) -> Result<std::sync::Arc<crate::plugin_manager::runtime::LoadedPlugin>, Response> {
    let installed = crate::plugin_manager::installed::list_cached(&state.cache, &admin.identity.org_db, &admin.db)
        .await
        .map_err(internal)?;
    let Some(row) = installed.into_iter().find(|row| row.plugin_name == plugin) else {
        return Err(failure(StatusCode::NOT_FOUND, "that plugin is not installed here"));
    };
    let core = state.core().await.map_err(internal)?;
    state
        .plugin_runtime
        .ensure_loaded(&core, plugin, &row.version)
        .await
        .map_err(|_| failure(StatusCode::SERVICE_UNAVAILABLE, "the plugin could not be loaded"))
}

fn address(org: &str, plugin: &str, name: &str) -> String {
    format!("/api/hooks/{org}/{plugin}/{name}")
}

async fn describe(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path(plugin): Path<String>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    let admin = match admin(&state, peer, &headers, &uri).await {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let loaded = match declared(&state, &admin, &plugin).await {
        Ok(loaded) => loaded,
        Err(response) => return response,
    };
    let mut response = match admin
        .db
        .query("SELECT * FROM inbound_hooks WHERE plugin = $plugin;")
        .bind(("plugin", plugin.clone()))
        .await
        .and_then(|r| r.check())
    {
        Ok(response) => response,
        Err(error) => return internal(error),
    };
    let rows: Vec<Value> = response.take(0).unwrap_or_default();
    let hooks: Vec<Value> = loaded
        .manifest
        .inbound
        .iter()
        .map(|def| {
            let row = rows.iter().find(|row| row.get("name").and_then(Value::as_str) == Some(def.name.as_str()));
            let field = |key: &str| row.and_then(|row| row.get(key)).cloned().unwrap_or(Value::Null);
            json!({
                "name": def.name,
                "function": def.function,
                "verify": def.verify,
                "header": def.header_name(),
                "address": address(&admin.identity.org_db, &plugin, &def.name),
                "has_secret": row.is_some(),
                "enabled": row.is_some_and(|row| row.get("enabled").and_then(Value::as_bool) == Some(true)),
                "received": field("received"),
                "rejected": field("rejected"),
                "last_received_at": field("last_received_at"),
                "last_rejected_at": field("last_rejected_at"),
                "last_rejected_reason": field("last_rejected_reason"),
            })
        })
        .collect();
    Json(json!({ "hooks": hooks })).into_response()
}

async fn make_secret(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path((plugin, name)): Path<(String, String)>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    let admin = match admin(&state, peer, &headers, &uri).await {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let loaded = match declared(&state, &admin, &plugin).await {
        Ok(loaded) => loaded,
        Err(response) => return response,
    };
    let Some(def) = loaded.manifest.inbound.iter().find(|hook| hook.name == name) else {
        return failure(StatusCode::NOT_FOUND, "the plugin declares no such hook");
    };
    let secret = crate::webhooks::generate_secret();
    let stored = match state.secrets() {
        Ok(secrets) => secrets.encrypt(&secret),
        Err(error) => return internal(error),
    };
    // A new hook starts with this secret; an existing one keeps its old secret signing for a day.
    let result = admin
        .db
        .query(format!(
            "LET $had = (SELECT VALUE secret FROM inbound_hooks WHERE plugin = $plugin AND name = $name LIMIT 1)[0]; \
             IF $had = NONE {{ CREATE inbound_hooks SET plugin = $plugin, name = $name, secret = $secret, created_by = $user; }} \
             ELSE {{ UPDATE inbound_hooks SET secret_prev = secret, secret_prev_until = time::now() + {}h, secret = $secret, enabled = true \
                WHERE plugin = $plugin AND name = $name; }}; \
             RETURN $had != NONE;",
            crate::webhooks::ROTATION_OVERLAP_HOURS
        ))
        .bind(("plugin", plugin.clone()))
        .bind(("name", name.clone()))
        .bind(("secret", stored))
        .bind(("user", admin.user.clone()))
        .await
        .and_then(|r| r.check());
    match result {
        Ok(mut response) => {
            let rotated: Option<bool> = response.take(2).ok().flatten();
            let rotated = rotated.unwrap_or(false);
            (
                if rotated { StatusCode::OK } else { StatusCode::CREATED },
                Json(json!({
                    "secret": secret,
                    "address": address(&admin.identity.org_db, &plugin, &name),
                    "verify": def.verify,
                    "header": def.header_name(),
                    "rotated": rotated,
                    "old_secret_works_for_hours": if rotated { json!(crate::webhooks::ROTATION_OVERLAP_HOURS) } else { Value::Null },
                    "note": "This is the only time the secret is shown.",
                })),
            )
                .into_response()
        }
        Err(error) => internal(error),
    }
}

#[derive(Debug, serde::Deserialize)]
struct SwitchBody {
    enabled: bool,
}

async fn switch(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path((plugin, name)): Path<(String, String)>,
    headers: HeaderMap,
    uri: axum::http::Uri,
    Json(body): Json<SwitchBody>,
) -> Response {
    let admin = match admin(&state, peer, &headers, &uri).await {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let result = admin
        .db
        .query("SELECT VALUE name FROM (UPDATE inbound_hooks SET enabled = $on WHERE plugin = $plugin AND name = $name);")
        .bind(("on", body.enabled))
        .bind(("plugin", plugin))
        .bind(("name", name.clone()))
        .await
        .and_then(|r| r.check());
    match result {
        Ok(mut response) => {
            let done: Vec<Value> = response.take(0).unwrap_or_default();
            if done.is_empty() {
                failure(StatusCode::NOT_FOUND, "that hook has no secret yet")
            } else {
                Json(json!({ "name": name, "enabled": body.enabled })).into_response()
            }
        }
        Err(error) => internal(error),
    }
}

async fn remove(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path((plugin, name)): Path<(String, String)>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    let admin = match admin(&state, peer, &headers, &uri).await {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let result = admin
        .db
        .query("SELECT VALUE name FROM (DELETE inbound_hooks WHERE plugin = $plugin AND name = $name RETURN BEFORE);")
        .bind(("plugin", plugin))
        .bind(("name", name.clone()))
        .await
        .and_then(|r| r.check());
    match result {
        Ok(mut response) => {
            let done: Vec<Value> = response.take(0).unwrap_or_default();
            if done.is_empty() {
                failure(StatusCode::NOT_FOUND, "that hook has no secret")
            } else {
                Json(json!({ "deleted": name })).into_response()
            }
        }
        Err(error) => internal(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(verify: &str, header: Option<&str>, tolerance: Option<u64>) -> PluginInboundDef {
        PluginInboundDef {
            name: "h".into(),
            function: "f".into(),
            verify: verify.into(),
            header: header.map(str::to_string),
            tolerance_secs: tolerance,
        }
    }

    fn secrets(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_token_must_match_a_secret_exactly() {
        let d = def("token", Some("x-api-key"), None);
        assert_eq!(verify(&d, &secrets(&["whsec_a"]), Some("whsec_a"), b"", 0), Ok(()));
        assert_eq!(verify(&d, &secrets(&["whsec_a"]), Some("whsec_b"), b"", 0), Err(Reject::Mismatch));
        assert_eq!(verify(&d, &secrets(&["whsec_a"]), Some("whsec_"), b"", 0), Err(Reject::Mismatch), "a prefix is not a match");
        assert_eq!(verify(&d, &secrets(&["whsec_a"]), None, b"", 0), Err(Reject::Missing));
        assert_eq!(verify(&d, &secrets(&["whsec_a"]), Some("  "), b"", 0), Err(Reject::Missing));
        assert_eq!(verify(&d, &[], Some("whsec_a"), b"", 0), Err(Reject::Mismatch), "no secret admits nobody");
        assert_eq!(verify(&d, &secrets(&["old", "new"]), Some("old"), b"", 0), Ok(()), "both secrets work during a rotation");
    }

    #[test]
    fn an_hmac_covers_the_raw_body() {
        let d = def("hmac-sha256", Some("x-signature"), None);
        let body = br#"{"amount":5}"#;
        let good = hmac_hex("whsec_k", body);
        assert_eq!(verify(&d, &secrets(&["whsec_k"]), Some(&good), body, 0), Ok(()));
        assert_eq!(verify(&d, &secrets(&["whsec_k"]), Some(&format!("sha256={good}")), body, 0), Ok(()), "the form many services use");
        assert_eq!(verify(&d, &secrets(&["whsec_k"]), Some(&good.to_uppercase()), body, 0), Ok(()), "hex case does not matter");
        assert_eq!(verify(&d, &secrets(&["whsec_k"]), Some(&good), br#"{"amount":6}"#, 0), Err(Reject::Mismatch), "another body");
        assert_eq!(verify(&d, &secrets(&["other"]), Some(&good), body, 0), Err(Reject::Mismatch), "another secret");
        assert_eq!(verify(&d, &secrets(&["whsec_k"]), Some("nothex"), body, 0), Err(Reject::Malformed));
        assert_eq!(verify(&d, &secrets(&["whsec_k"]), Some("abcd"), body, 0), Err(Reject::Malformed), "too short");
        assert_eq!(verify(&d, &secrets(&["whsec_k"]), None, body, 0), Err(Reject::Missing));
        // The known answer, so any language can be checked against it.
        assert_eq!(
            hmac_hex("key", b"The quick brown fox jumps over the lazy dog"),
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
    }

    #[test]
    fn our_own_scheme_signs_the_time_and_refuses_a_replay() {
        let d = def("aether", None, Some(300));
        let body = b"{\"a\":1}";
        let header = |secret: &str, t: i64| crate::webhooks::signature_header(t, &[secret.to_string()], body);
        assert_eq!(verify(&d, &secrets(&["s"]), Some(&header("s", 1000)), body, 1000), Ok(()));
        assert_eq!(verify(&d, &secrets(&["s"]), Some(&header("s", 1000)), body, 1299), Ok(()), "within the tolerance");
        assert_eq!(verify(&d, &secrets(&["s"]), Some(&header("s", 1000)), body, 1301), Err(Reject::Stale), "too old: a replay");
        assert_eq!(verify(&d, &secrets(&["s"]), Some(&header("s", 1000)), body, 699), Err(Reject::Stale), "from the future");
        assert_eq!(verify(&d, &secrets(&["s"]), Some(&header("wrong", 1000)), body, 1000), Err(Reject::Mismatch));
        assert_eq!(
            verify(&d, &secrets(&["s"]), Some(&header("wrong", 1000)), body, 99999),
            Err(Reject::Mismatch),
            "a wrong secret is a mismatch whatever the time says, so a stranger learns nothing about the clock"
        );
        // Changing the time in the header breaks the signature.
        let forged = header("s", 1000).replace("t=1000", "t=5000");
        assert_eq!(verify(&d, &secrets(&["s"]), Some(&forged), body, 5000), Err(Reject::Mismatch));
        assert_eq!(verify(&d, &secrets(&["s"]), Some("v1=abc"), body, 0), Err(Reject::Malformed), "no time");
        assert_eq!(verify(&d, &secrets(&["s"]), Some("t=1"), body, 0), Err(Reject::Malformed), "no signature");
        assert_eq!(verify(&d, &secrets(&["s"]), Some("garbage"), body, 0), Err(Reject::Malformed));
        // Two signatures (during a rotation): either secret is enough.
        let two = crate::webhooks::signature_header(1000, &["new".into(), "old".into()], body);
        assert_eq!(verify(&d, &secrets(&["old"]), Some(&two), body, 1000), Ok(()));
        assert_eq!(verify(&d, &secrets(&["new"]), Some(&two), body, 1000), Ok(()));
        // A tighter tolerance.
        assert_eq!(verify(&def("aether", None, Some(10)), &secrets(&["s"]), Some(&header("s", 1000)), body, 1011), Err(Reject::Stale));
    }

    #[test]
    fn comparison_does_not_stop_at_the_first_difference() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(!constant_time_eq(b"", b"a"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn the_function_never_sees_credentials_or_the_signature() {
        let mut headers = HeaderMap::new();
        for (name, value) in [
            ("content-type", "application/json"),
            ("x-request-id", "r1"),
            ("authorization", "Bearer x"),
            ("cookie", "aether_session=abc"),
            ("x-signature", "deadbeef"),
            ("x-hub-signature-256", "sha256=ab"),
            ("x-api-key", "k"),
            ("x-github-event", "push"),
        ] {
            headers.insert(axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(), value.parse().unwrap());
        }
        let seen = visible_headers(&headers, "x-signature");
        let mut names: Vec<&str> = seen.keys().map(String::as_str).collect();
        names.sort();
        assert_eq!(names, vec!["content-type", "x-github-event", "x-request-id"]);
    }
}
