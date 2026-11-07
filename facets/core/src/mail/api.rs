//! Inbound mail over HTTP (see `docs/architecture/inbound_mail.md`).
//!
//! ```text
//! POST  /api/mail/<organization>/<provider>     the mail provider posts each message it received
//! GET   /api/ui/mail                            an administrator: address, state, counters, routes
//! POST  /api/ui/mail/secret                     make or rotate the provider's secret (shown once)
//! PATCH /api/ui/mail { enabled }                switch intake off or on
//! GET   /api/ui/mail/messages?status=&limit=    what came in and what became of it
//! POST  /api/ui/mail/messages/<key>/assign      give an unmatched message to a plugin's alias
//! POST  /api/ui/mail/messages/<key>/discard     set an unmatched message aside
//! ```
//!
//! The provider proves itself with the organization's secret, either in `X-Aether-Mail-Token` or as the
//! password of HTTP Basic authentication (`https://mail:<secret>@host/api/mail/<org>/postmark`, the way
//! providers that cannot set headers are configured). The check comes **before** anything is parsed or stored.
//! An organization that does not exist, has no secret or has switched intake off answers `404`; a wrong secret
//! answers `401` and is counted.

use std::net::SocketAddr;

use axum::{
    Extension, Json, Router,
    body::Body,
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::Engine as _;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{message, service};
use crate::inbound::constant_time_eq;
use crate::state::AppState;
use crate::webhooks_api::{admin, failure, internal};

/// Largest message accepted (the provider's own limits are smaller than this).
pub const MAX_BODY_BYTES: usize = 25 * 1024 * 1024;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/mail/{org}/{provider}", post(receive))
        .route("/api/ui/mail", get(describe).patch(switch))
        .route("/api/ui/mail/secret", post(make_secret))
        .route("/api/ui/mail/messages", get(messages))
        .route("/api/ui/mail/messages/{key}/assign", post(assign))
        .route("/api/ui/mail/messages/{key}/discard", post(discard))
}

fn not_found() -> Response {
    StatusCode::NOT_FOUND.into_response()
}

/// The secret a request presents: the token header, or the password of Basic authentication.
fn presented(headers: &HeaderMap) -> Option<String> {
    if let Some(token) = headers.get("x-aether-mail-token").and_then(|v| v.to_str().ok()) {
        return Some(token.trim().to_string());
    }
    let basic = headers.get("authorization")?.to_str().ok()?.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD.decode(basic.trim()).ok()?;
    let text = String::from_utf8(decoded).ok()?;
    text.split_once(':').map(|(_, password)| password.to_string())
}

async fn receive(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path((org, provider)): Path<(String, String)>,
    headers: HeaderMap,
    request: Request<Body>,
) -> Response {
    let peer_ip = peer.map(|Extension(ConnectInfo(address))| address.ip());
    let trusted = state.config.server.as_ref().map(|server| server.trusted_proxies.as_slice()).unwrap_or_default();
    let ip = crate::access::ip::client_ip(&headers, peer_ip, trusted);
    if !state.limits.request_limiter.allow(ip) {
        return crate::access::identity::with_retry_after(failure(StatusCode::TOO_MANY_REQUESTS, "too many requests"));
    }
    match crate::access::org_databases::list_cached(&state).await {
        Ok(names) if names.iter().any(|candidate| *candidate == org) => {}
        Ok(_) => return not_found(),
        Err(error) => return internal(error),
    }
    if !message::PROVIDERS.contains(&provider.as_str()) {
        return not_found();
    }
    let Ok(db) = state.org(&org).await else { return not_found() };
    let mut response = match db.query("SELECT * FROM mail_inbox LIMIT 1;").await.and_then(|r| r.check()) {
        Ok(response) => response,
        Err(error) => return internal(error),
    };
    let rows: Vec<Value> = response.take(0).unwrap_or_default();
    let Some(inbox) = rows
        .first()
        .filter(|row| row.get("enabled").and_then(Value::as_bool) == Some(true) && row.get("secret").is_some_and(Value::is_string))
    else {
        return not_found();
    };

    let Ok(secrets_box) = state.secrets() else {
        return failure(StatusCode::INTERNAL_SERVER_ERROR, "the encryption key is unavailable");
    };
    let mut secrets = Vec::new();
    if let Some(current) = inbox.get("secret").and_then(Value::as_str).and_then(|s| secrets_box.decrypt(s).ok()) {
        secrets.push(current);
    }
    let rotating = inbox
        .get("secret_prev_until")
        .and_then(Value::as_str)
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        .is_some_and(|until| until > chrono::Utc::now());
    if rotating
        && let Some(previous) = inbox.get("secret_prev").and_then(Value::as_str).and_then(|s| secrets_box.decrypt(s).ok())
    {
        secrets.push(previous);
    }
    let given = presented(&headers);
    let allowed = given
        .as_ref()
        .is_some_and(|given| secrets.iter().fold(false, |ok, secret| constant_time_eq(secret.as_bytes(), given.as_bytes()) | ok));
    if !allowed {
        let why = if given.is_none() { "no secret was presented" } else { "the secret did not match" };
        log::warn!("{org}: inbound mail refused a call from {ip:?}: {why}");
        let _ = db
            .query("UPDATE mail_inbox SET rejected += 1, last_rejected_at = time::now(), last_rejected_reason = $why;")
            .bind(("why", why.to_string()))
            .await;
        return failure(StatusCode::UNAUTHORIZED, "invalid secret");
    }

    let body = match axum::body::to_bytes(request.into_body(), MAX_BODY_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => return failure(StatusCode::PAYLOAD_TOO_LARGE, "the message is larger than 25 MiB"),
    };
    let parsed = match serde_json::from_slice::<Value>(&body)
        .map_err(|_| message::ParseError("the body is not JSON".into()))
        .and_then(|json| message::parse(&provider, &json))
    {
        Ok(parsed) => parsed,
        Err(error) => {
            // A message that cannot be read will not become readable: the provider should not retry.
            log::warn!("{org}: inbound mail from {provider} could not be read: {error}");
            return failure(StatusCode::UNPROCESSABLE_ENTITY, error.to_string());
        }
    };
    let _ = db.query("UPDATE mail_inbox SET received += 1, last_received_at = time::now();").await;
    match service::accept(&state, &org, &db, parsed).await {
        Ok(outcome) => Json(json!({ "ok": true, "outcome": outcome.word() })).into_response(),
        Err(error) => {
            log::error!("{org}: inbound mail could not be handled: {error}");
            failure(StatusCode::INTERNAL_SERVER_ERROR, "the message could not be handled; try again")
        }
    }
}

async fn describe(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    let admin = match admin(&state, peer, &headers, &uri).await {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let org = admin.identity.org_db.clone();
    let inbox: Vec<Value> = match admin.db.query("SELECT * FROM mail_inbox LIMIT 1;").await.and_then(|r| r.check()) {
        Ok(mut r) => r.take(0).unwrap_or_default(),
        Err(error) => return internal(error),
    };
    let inbox = inbox.first();
    let field = |key: &str| inbox.and_then(|row| row.get(key)).cloned().unwrap_or(Value::Null);
    let counts: Vec<Value> = match admin
        .db
        .query("SELECT status, count() AS n FROM inbound_messages GROUP BY status;")
        .await
        .and_then(|r| r.check())
    {
        Ok(mut r) => r.take(0).unwrap_or_default(),
        Err(error) => return internal(error),
    };
    let limits = service::limits(&state, &org).await;
    let routes: Vec<Value> = service::routes(&state, &org, &admin.db)
        .await
        .into_iter()
        .map(|(plugin, route)| {
            json!({
                "plugin": plugin,
                "alias": route.alias,
                "function": route.function,
                "model": route.model,
                "require": route.require,
                "address": limits.domain.as_ref().map(|domain| format!("{}@{domain}", route.alias)),
            })
        })
        .collect();
    Json(json!({
        "address": format!("/api/mail/{org}/<provider>"),
        "providers": message::PROVIDERS,
        "domain": limits.domain,
        "has_secret": inbox.is_some_and(|row| row.get("secret").is_some_and(Value::is_string)),
        "enabled": inbox.is_some_and(|row| row.get("enabled").and_then(Value::as_bool) == Some(true)),
        "received": field("received"),
        "rejected": field("rejected"),
        "last_received_at": field("last_received_at"),
        "last_rejected_at": field("last_rejected_at"),
        "last_rejected_reason": field("last_rejected_reason"),
        "messages": counts,
        "routes": routes,
    }))
    .into_response()
}

async fn make_secret(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    let admin = match admin(&state, peer, &headers, &uri).await {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let secret = crate::webhooks::generate_secret();
    let stored = match state.secrets() {
        Ok(secrets) => secrets.encrypt(&secret),
        Err(error) => return internal(error),
    };
    let result = admin
        .db
        .query(format!(
            "LET $had = (SELECT VALUE secret FROM mail_inbox LIMIT 1)[0]; \
             IF $had = NONE {{ CREATE mail_inbox SET secret = $secret, enabled = true; }} \
             ELSE {{ UPDATE mail_inbox SET secret_prev = secret, secret_prev_until = time::now() + {}h, secret = $secret, enabled = true; }}; \
             RETURN $had != NONE;",
            crate::webhooks::ROTATION_OVERLAP_HOURS
        ))
        .bind(("secret", stored))
        .await
        .and_then(|r| r.check());
    match result {
        Ok(mut response) => {
            let rotated = response.take::<Option<bool>>(2).ok().flatten().unwrap_or(false);
            (
                if rotated { StatusCode::OK } else { StatusCode::CREATED },
                Json(json!({
                    "secret": secret,
                    "header": "X-Aether-Mail-Token",
                    "or": "the password of HTTP Basic authentication (any user name)",
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

#[derive(Debug, Deserialize)]
struct SwitchBody {
    enabled: bool,
}

async fn switch(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: axum::http::Uri,
    Json(body): Json<SwitchBody>,
) -> Response {
    let admin = match admin(&state, peer, &headers, &uri).await {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    match admin
        .db
        .query("SELECT VALUE enabled FROM (UPDATE mail_inbox SET enabled = $on);")
        .bind(("on", body.enabled))
        .await
        .and_then(|r| r.check())
    {
        Ok(mut response) => {
            let done: Vec<Value> = response.take(0).unwrap_or_default();
            if done.is_empty() {
                failure(StatusCode::NOT_FOUND, "no secret has been made yet")
            } else {
                Json(json!({ "enabled": body.enabled })).into_response()
            }
        }
        Err(error) => internal(error),
    }
}

#[derive(Debug, Deserialize)]
struct MessagesQuery {
    status: Option<String>,
    limit: Option<u32>,
}

async fn messages(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Query(query): Query<MessagesQuery>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    let admin = match admin(&state, peer, &headers, &uri).await {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let status = query.status.filter(|s| ["received", "routed", "replied", "unmatched", "ignored", "failed"].contains(&s.as_str()));
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let result = admin
        .db
        .query(format!(
            "SELECT meta::id(id) AS key, message_id, alias, sender, subject, status, reason, plugin, route_function AS function, model_id, record_key, received_at, handled_at, \
             array::len(message.attachments ?? []) AS attachments, string::slice(message.text ?? '', 0, 300) AS preview \
             FROM inbound_messages {} ORDER BY received_at DESC LIMIT {limit};",
            if status.is_some() { "WHERE status = $status" } else { "" }
        ))
        .bind(("status", status.unwrap_or_default()))
        .await
        .and_then(|r| r.check());
    match result {
        Ok(mut response) => Json(json!({ "messages": response.take::<Vec<Value>>(0).unwrap_or_default() })).into_response(),
        Err(error) => internal(error),
    }
}

#[derive(Debug, Deserialize)]
struct AssignBody {
    plugin: String,
    alias: String,
}

fn plain_key(key: &str) -> bool {
    !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

async fn assign(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path(key): Path<String>,
    headers: HeaderMap,
    uri: axum::http::Uri,
    Json(body): Json<AssignBody>,
) -> Response {
    let admin = match admin(&state, peer, &headers, &uri).await {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    if !plain_key(&key) {
        return failure(StatusCode::BAD_REQUEST, "that is not a message key");
    }
    match service::assign(&state, &admin.identity.org_db, &admin.db, &key, &body.plugin, &body.alias).await {
        Ok(()) => Json(json!({ "assigned": key, "plugin": body.plugin, "alias": body.alias })).into_response(),
        Err(reason) => failure(StatusCode::UNPROCESSABLE_ENTITY, reason),
    }
}

async fn discard(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path(key): Path<String>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    let admin = match admin(&state, peer, &headers, &uri).await {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    if !plain_key(&key) {
        return failure(StatusCode::BAD_REQUEST, "that is not a message key");
    }
    let result = admin
        .db
        .query(
            "SELECT VALUE meta::id(id) FROM (UPDATE type::record('inbound_messages', $k) SET status = 'ignored', \
             reason = 'set aside by an administrator', handled_at = time::now() WHERE status = 'unmatched');",
        )
        .bind(("k", key.clone()))
        .await
        .and_then(|r| r.check());
    match result {
        Ok(mut response) => {
            let done: Vec<Value> = response.take(0).unwrap_or_default();
            if done.is_empty() {
                failure(StatusCode::NOT_FOUND, "there is no unmatched message with that key")
            } else {
                Json(json!({ "discarded": key })).into_response()
            }
        }
        Err(error) => internal(error),
    }
}
