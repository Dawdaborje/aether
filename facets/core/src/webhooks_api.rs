//! Managing webhooks: an administrator subscribes addresses to events, sees every delivery, and sends
//! one again (see [`crate::webhooks`] and `docs/architecture/api_and_webhooks.md`).
//!
//! ```text
//! GET    /api/ui/webhooks                          the subscriptions (never their secrets)
//! POST   /api/ui/webhooks { name, url, events, conditions? }   -> 201 { id, secret }  (the secret is shown once)
//! PATCH  /api/ui/webhooks/{id} { name?, url?, events?, conditions?, active? }
//! DELETE /api/ui/webhooks/{id}
//! POST   /api/ui/webhooks/{id}/rotate              -> { secret }; the old one signs too for a day
//! POST   /api/ui/webhooks/{id}/test                sends an `aether.ping` event now
//! GET    /api/ui/webhooks/{id}/deliveries?limit=50
//! POST   /api/ui/webhooks/deliveries/{ulid}/replay
//! ```
//!
//! Only organization administrators: a webhook sends the organization's data out of it.

use std::net::SocketAddr;

use axum::{
    Extension, Json, Router,
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::access::audit::Actor;
use crate::access::identity::{Identity, identify};
use crate::state::{AppState, Databases};
use crate::webhooks::{self, MAX_EVENTS_PER_SUBSCRIPTION, MAX_SUBSCRIPTIONS};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/ui/webhooks", get(list).post(create))
        .route("/api/ui/webhooks/{id}", axum::routing::patch(update).delete(remove))
        .route("/api/ui/webhooks/{id}/rotate", post(rotate))
        .route("/api/ui/webhooks/{id}/test", post(test))
        .route("/api/ui/webhooks/{id}/deliveries", get(deliveries))
        .route("/api/ui/webhooks/deliveries/{ulid}/replay", post(replay))
}

pub(crate) fn failure(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": message.into() }))).into_response()
}

pub(crate) fn internal(error: impl std::fmt::Display) -> Response {
    log::error!("webhooks: {error}");
    failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
}

/// An organization administrator, with the organization's database.
pub(crate) struct Admin {
    pub(crate) identity: Identity,
    pub(crate) user: String,
    pub(crate) db: crate::databases::Db,
}

pub(crate) async fn admin(
    state: &AppState,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<Admin, Response> {
    let peer_ip = peer.map(|Extension(ConnectInfo(address))| address.ip());
    let identity = identify(state, headers, peer_ip, uri).await.map_err(|error| {
        let (status, message) = error.http();
        failure(status, message)
    })?;
    let (Actor::User(user), Some(_)) = (&identity.actor, &identity.session) else {
        return Err(failure(StatusCode::UNAUTHORIZED, "sign in to manage webhooks"));
    };
    let user = user.clone();
    let db = state.org(&identity.org_db).await.map_err(internal)?;
    let roles = crate::roles::roles_of_cached(&state.cache, &identity.org_db, &db, &user)
        .await
        .map_err(internal)?;
    if !roles.iter().any(|role| role == crate::roles::ORG_ADMIN) {
        return Err(failure(StatusCode::FORBIDDEN, "only an organization administrator manages webhooks"));
    }
    Ok(Admin { identity, user, db })
}

async fn allow_http(state: &Databases, admin: &Admin) -> bool {
    let core = state.core().await.ok();
    crate::kernel::host::settings_read::effective_in(&admin.db, core.as_ref(), &["webhooks.allow_http"])
        .await
        .get("webhooks.allow_http")
        == Some(&Value::Bool(true))
}

/// What is wrong with the events of a subscription: they are full names of installed plugins.
async fn events_problem(admin: &Admin, events: &[String]) -> Result<Option<String>, Response> {
    if events.is_empty() || events.len() > MAX_EVENTS_PER_SUBSCRIPTION {
        return Ok(Some(format!("name 1 to {MAX_EVENTS_PER_SUBSCRIPTION} events")));
    }
    if let Some(bad) = events.iter().find(|e| !crate::plugin_events::is_full_name(e)) {
        return Ok(Some(format!("`{bad}` is not an event name: write <plugin>.<event>, such as crm.lead_won")));
    }
    let mut response = admin
        .db
        .query("SELECT VALUE plugin_name FROM installed_plugins;")
        .await
        .and_then(|r| r.check())
        .map_err(internal)?;
    let installed: Vec<String> = response.take(0).map_err(internal)?;
    for event in events {
        let source = event.split_once('.').map_or("", |(source, _)| source);
        if !installed.iter().any(|name| name == source) {
            return Ok(Some(format!("`{source}` is not installed in this organization")));
        }
    }
    Ok(None)
}

fn shown(row: &Value) -> Value {
    let mut out = row.clone();
    if let Some(map) = out.as_object_mut() {
        map.remove("secret");
        let rotating = map
            .remove("secret_prev_until")
            .and_then(|t| t.as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()))
            .is_some_and(|until| until > chrono::Utc::now());
        map.remove("secret_prev");
        map.insert("old_secret_still_signs".into(), json!(rotating));
    }
    out
}

async fn list(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let admin = match admin(&state, peer, &headers, &uri).await {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let result = admin
        .db
        .query("SELECT *, <string> record::id(id) AS id FROM webhook_subscriptions ORDER BY created_at;")
        .await
        .and_then(|r| r.check());
    match result {
        Ok(mut response) => match response.take::<Vec<Value>>(0) {
            Ok(rows) => Json(json!({ "subscriptions": rows.iter().map(shown).collect::<Vec<_>>() })).into_response(),
            Err(error) => internal(error),
        },
        Err(error) => internal(error),
    }
}

#[derive(Debug, Deserialize)]
struct CreateBody {
    name: String,
    url: String,
    events: Vec<String>,
    #[serde(default)]
    conditions: Option<Value>,
}

fn conditions_problem(conditions: &Option<Value>) -> Option<&'static str> {
    match conditions {
        None | Some(Value::Null) => None,
        Some(Value::Object(map)) if map.len() <= 20 && map.values().all(|v| !v.is_object() && !v.is_array()) => None,
        _ => Some("`conditions` is an object of field = value, up to 20, with plain values"),
    }
}

async fn create(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
    Json(body): Json<CreateBody>,
) -> Response {
    let admin = match admin(&state, peer, &headers, &uri).await {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let name = body.name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return failure(StatusCode::UNPROCESSABLE_ENTITY, "name the webhook, in at most 80 characters");
    }
    if let Some(problem) = webhooks::url_problem(&body.url, allow_http(state.databases(), &admin).await) {
        return failure(StatusCode::UNPROCESSABLE_ENTITY, problem);
    }
    if let Some(problem) = conditions_problem(&body.conditions) {
        return failure(StatusCode::UNPROCESSABLE_ENTITY, problem);
    }
    match events_problem(&admin, &body.events).await {
        Ok(None) => {}
        Ok(Some(problem)) => return failure(StatusCode::UNPROCESSABLE_ENTITY, problem),
        Err(response) => return response,
    }
    let mut counted = match admin
        .db
        .query("SELECT count() AS count FROM webhook_subscriptions GROUP ALL;")
        .await
        .and_then(|r| r.check())
    {
        Ok(response) => response,
        Err(error) => return internal(error),
    };
    let existing: Vec<Value> = counted.take(0).unwrap_or_default();
    if existing.first().and_then(|r| r.get("count")).and_then(Value::as_u64).unwrap_or(0) >= MAX_SUBSCRIPTIONS as u64 {
        return failure(StatusCode::UNPROCESSABLE_ENTITY, format!("at most {MAX_SUBSCRIPTIONS} webhooks"));
    }
    let secret = webhooks::generate_secret();
    let stored = match state.secrets() {
        Ok(secrets) => secrets.encrypt(&secret),
        Err(error) => return internal(error),
    };
    let mut events = body.events.clone();
    events.sort();
    events.dedup();
    let made = admin
        .db
        .query(
            "CREATE webhook_subscriptions SET name = $name, url = $url, events = $events, conditions = $conditions, \
             secret = $secret, created_by = $user RETURN <string> record::id(id) AS id;",
        )
        .bind(("name", name.to_string()))
        .bind(("url", body.url.clone()))
        .bind(("events", events))
        .bind(("conditions", body.conditions.clone().filter(|c| !c.is_null())))
        .bind(("secret", stored))
        .bind(("user", admin.user.clone()))
        .await
        .and_then(|r| r.check());
    match made {
        Ok(mut response) => {
            let rows: Vec<Value> = response.take(0).unwrap_or_default();
            let id = rows.first().and_then(|r| r.get("id")).cloned().unwrap_or(Value::Null);
            log::info!("{} webhook `{name}` made by {}", admin.identity.org_db, admin.user);
            (
                StatusCode::CREATED,
                Json(json!({ "id": id, "secret": secret, "note": "This is the only time the secret is shown." })),
            )
                .into_response()
        }
        Err(error) => internal(error),
    }
}

#[derive(Debug, Deserialize)]
struct UpdateBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    events: Option<Vec<String>>,
    #[serde(default)]
    conditions: Option<Value>,
    #[serde(default)]
    active: Option<bool>,
}

async fn update(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    Json(body): Json<UpdateBody>,
) -> Response {
    let admin = match admin(&state, peer, &headers, &uri).await {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let mut sets = Vec::new();
    let mut query_binds: Vec<(&str, Value)> = Vec::new();
    if let Some(name) = &body.name {
        if name.trim().is_empty() || name.chars().count() > 80 {
            return failure(StatusCode::UNPROCESSABLE_ENTITY, "name the webhook, in at most 80 characters");
        }
        sets.push("name = $name");
        query_binds.push(("name", json!(name.trim())));
    }
    if let Some(url) = &body.url {
        if let Some(problem) = webhooks::url_problem(url, allow_http(state.databases(), &admin).await) {
            return failure(StatusCode::UNPROCESSABLE_ENTITY, problem);
        }
        sets.push("url = $url");
        query_binds.push(("url", json!(url)));
    }
    if let Some(events) = &body.events {
        match events_problem(&admin, events).await {
            Ok(None) => {}
            Ok(Some(problem)) => return failure(StatusCode::UNPROCESSABLE_ENTITY, problem),
            Err(response) => return response,
        }
        let mut events = events.clone();
        events.sort();
        events.dedup();
        sets.push("events = $events");
        query_binds.push(("events", json!(events)));
    }
    if body.conditions.is_some() {
        if let Some(problem) = conditions_problem(&body.conditions) {
            return failure(StatusCode::UNPROCESSABLE_ENTITY, problem);
        }
        sets.push("conditions = $conditions");
        query_binds.push(("conditions", body.conditions.clone().filter(|c| !c.is_null()).unwrap_or(Value::Null)));
    }
    match body.active {
        // Switching it on again is a decision that the receiver is fixed: the count starts over.
        Some(true) => sets.push("active = true, failures = 0, disabled_reason = NONE"),
        Some(false) => {
            sets.push("active = false, disabled_reason = 'switched off by an administrator'");
        }
        None => {}
    }
    if sets.is_empty() {
        return failure(StatusCode::UNPROCESSABLE_ENTITY, "send something to change");
    }
    let mut query = admin
        .db
        .query(format!(
            "SELECT VALUE <string> record::id(id) FROM (UPDATE type::record('webhook_subscriptions', $id) SET {});",
            sets.join(", ")
        ))
        .bind(("id", id.clone()));
    for (name, value) in query_binds {
        query = query.bind((name.to_string(), value));
    }
    match query.await.and_then(|r| r.check()) {
        Ok(mut response) => {
            let done: Vec<Value> = response.take(0).unwrap_or_default();
            if done.is_empty() {
                failure(StatusCode::NOT_FOUND, "no such webhook")
            } else {
                Json(json!({ "updated": id })).into_response()
            }
        }
        Err(error) => internal(error),
    }
}

async fn remove(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let admin = match admin(&state, peer, &headers, &uri).await {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let result = admin
        .db
        .query("SELECT VALUE <string> record::id(id) FROM (DELETE type::record('webhook_subscriptions', $id) RETURN BEFORE);")
        .bind(("id", id.clone()))
        .await
        .and_then(|r| r.check());
    match result {
        Ok(mut response) => {
            let done: Vec<Value> = response.take(0).unwrap_or_default();
            if done.is_empty() {
                failure(StatusCode::NOT_FOUND, "no such webhook")
            } else {
                Json(json!({ "deleted": id })).into_response()
            }
        }
        Err(error) => internal(error),
    }
}

async fn rotate(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let admin = match admin(&state, peer, &headers, &uri).await {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let secret = webhooks::generate_secret();
    let stored = match state.secrets() {
        Ok(secrets) => secrets.encrypt(&secret),
        Err(error) => return internal(error),
    };
    // The old secret keeps signing for a day, so a receiver can switch over without dropping events.
    let result = admin
        .db
        .query(format!(
            "SELECT VALUE <string> record::id(id) FROM (UPDATE type::record('webhook_subscriptions', $id) SET \
             secret_prev = secret, secret_prev_until = time::now() + {}h, secret = $secret);",
            webhooks::ROTATION_OVERLAP_HOURS
        ))
        .bind(("id", id))
        .bind(("secret", stored))
        .await
        .and_then(|r| r.check());
    match result {
        Ok(mut response) => {
            let done: Vec<Value> = response.take(0).unwrap_or_default();
            if done.is_empty() {
                failure(StatusCode::NOT_FOUND, "no such webhook")
            } else {
                Json(json!({
                    "secret": secret,
                    "old_secret_signs_for_hours": webhooks::ROTATION_OVERLAP_HOURS,
                    "note": "This is the only time the secret is shown.",
                }))
                .into_response()
            }
        }
        Err(error) => internal(error),
    }
}

async fn test(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let admin = match admin(&state, peer, &headers, &uri).await {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let mut response = match admin
        .db
        .query("SELECT VALUE <string> record::id(id) FROM webhook_subscriptions WHERE id = type::record('webhook_subscriptions', $id);")
        .bind(("id", id.clone()))
        .await
        .and_then(|r| r.check())
    {
        Ok(response) => response,
        Err(error) => return internal(error),
    };
    let found: Vec<Value> = response.take(0).unwrap_or_default();
    if found.is_empty() {
        return failure(StatusCode::NOT_FOUND, "no such webhook");
    }
    let data = json!({ "ping": true, "webhook": id }).as_object().cloned().unwrap_or_default();
    // A test is tried once: the answer is what the administrator wants to see.
    let retry = webhooks::Retry { attempts: 1, backoff_secs: 1 };
    match webhooks::queue_delivery(&admin.db, &id, "aether.ping", &data, None, retry, Some(&admin.identity.audit.request_id)).await {
        Ok(true) => {
            state.scheduler.wake(&admin.identity.org_db);
            Json(json!({ "queued": true })).into_response()
        }
        Ok(false) => failure(StatusCode::INTERNAL_SERVER_ERROR, "the test could not be queued"),
        Err(error) => internal(error),
    }
}

#[derive(Debug, Deserialize)]
struct PageQuery {
    #[serde(default)]
    limit: Option<u32>,
}

async fn deliveries(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path(id): Path<String>,
    Query(query): Query<PageQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let admin = match admin(&state, peer, &headers, &uri).await {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let result = admin
        .db
        .query(
            "SELECT ulid, event, status, attempts, http_status, error, response_head, duration_ms, replay_of, created_at, last_attempt_at \
             FROM webhook_deliveries WHERE subscription = $id ORDER BY created_at DESC LIMIT $limit;",
        )
        .bind(("id", id))
        .bind(("limit", limit))
        .await
        .and_then(|r| r.check());
    match result {
        Ok(mut response) => match response.take::<Vec<Value>>(0) {
            Ok(rows) => Json(json!({ "deliveries": rows })).into_response(),
            Err(error) => internal(error),
        },
        Err(error) => internal(error),
    }
}

async fn replay(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path(ulid): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let admin = match admin(&state, peer, &headers, &uri).await {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let mut response = match admin
        .db
        .query("SELECT * FROM webhook_deliveries WHERE ulid = $ulid LIMIT 1;")
        .bind(("ulid", ulid.clone()))
        .await
        .and_then(|r| r.check())
    {
        Ok(response) => response,
        Err(error) => return internal(error),
    };
    let found: Vec<Value> = response.take(0).unwrap_or_default();
    let Some(delivery) = found.first() else {
        return failure(StatusCode::NOT_FOUND, "no such delivery");
    };
    let subscription = delivery.get("subscription").and_then(Value::as_str).unwrap_or("").to_string();
    let mut response = match admin
        .db
        .query("SELECT active FROM type::record('webhook_subscriptions', $id);")
        .bind(("id", subscription.clone()))
        .await
        .and_then(|r| r.check())
    {
        Ok(response) => response,
        Err(error) => return internal(error),
    };
    let subs: Vec<Value> = response.take(0).unwrap_or_default();
    match subs.first().and_then(|s| s.get("active")).and_then(Value::as_bool) {
        None => return failure(StatusCode::NOT_FOUND, "the webhook was removed"),
        Some(false) => return failure(StatusCode::CONFLICT, "the webhook is switched off; switch it on first"),
        Some(true) => {}
    }
    let data = delivery.get("data").and_then(Value::as_object).cloned().unwrap_or_default();
    let event = delivery.get("event").and_then(Value::as_str).unwrap_or("").to_string();
    let core = state.core().await.ok();
    let retry = webhooks::retry_of(&admin.db, core.as_ref()).await;
    match webhooks::queue_delivery(&admin.db, &subscription, &event, &data, Some(&ulid), retry, Some(&admin.identity.audit.request_id)).await {
        Ok(true) => {
            state.scheduler.wake(&admin.identity.org_db);
            Json(json!({ "queued": true, "replay_of": ulid })).into_response()
        }
        Ok(false) => failure(StatusCode::UNPROCESSABLE_ENTITY, "that delivery cannot be sent again"),
        Err(error) => internal(error),
    }
}
