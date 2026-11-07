//! A person's own login for a bridge that works on their behalf (their mailbox on a JMAP server).
//!
//! ```text
//! GET     /api/ui/bridge-credentials/<bridge>      { bridge, connected, username }   (the secret is never sent back)
//! PUT     /api/ui/bridge-credentials/<bridge>      { username, secret }  checks the login with the bridge, then keeps it
//! DELETE  /api/ui/bridge-credentials/<bridge>      forget it
//! ```
//!
//! The secret is stored encrypted in the organization's database, per person and bridge. When a plugin calls such a bridge
//! (`bridges::PER_USER`), the kernel looks up the **caller's** login and hands it to the bridge as `_credential`; the plugin
//! never sees it and cannot name another person. Only the person themselves can set or remove their login.

use std::net::SocketAddr;

use axum::{
    Extension, Json, Router,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::access::audit::Actor;
use crate::access::identity::identify;
use crate::state::AppState;
use crate::webhooks_api::{failure, internal};

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/ui/bridge-credentials/{bridge}", get(show).put(save).delete(forget))
}

/// The signed-in person and the organization's database, or the response that refuses.
async fn member(state: &AppState, peer: Option<Extension<ConnectInfo<SocketAddr>>>, headers: &HeaderMap, uri: &Uri, bridge: &str) -> Result<(String, String), Response> {
    if !crate::bridges::PER_USER.contains(&bridge) {
        return Err(failure(StatusCode::NOT_FOUND, "that bridge does not use a login of your own"));
    }
    let peer_ip = peer.map(|Extension(ConnectInfo(address))| address.ip());
    let identity = identify(state, headers, peer_ip, uri).await.map_err(|error| {
        let (status, message) = error.http();
        failure(status, message)
    })?;
    match (&identity.actor, &identity.session) {
        (Actor::User(user), Some(_)) if !identity.foreign_user => Ok((identity.org_db.clone(), user.clone())),
        _ => Err(failure(StatusCode::UNAUTHORIZED, "sign in first")),
    }
}

/// The person's login for `bridge`: `(username, secret)`.
pub async fn load(state: &AppState, org_db: &str, user: &str, bridge: &str) -> Result<Option<(String, String)>, String> {
    let db = state.org(org_db).await.map_err(|e| e.to_string())?;
    let mut response = db
        .query("SELECT username, secret FROM user_bridge_credentials WHERE user = $user AND bridge = $bridge LIMIT 1;")
        .bind(("user", user.to_string()))
        .bind(("bridge", bridge.to_string()))
        .await
        .and_then(|r| r.check())
        .map_err(|e| e.to_string())?;
    let rows: Vec<Value> = response.take(0).map_err(|e| e.to_string())?;
    let Some(row) = rows.first() else { return Ok(None) };
    let secret = row["secret"].as_str().unwrap_or_default();
    let plain = state.secrets().map_err(|e| e.to_string())?.decrypt(secret).map_err(|e| e.to_string())?;
    Ok(Some((row["username"].as_str().unwrap_or_default().to_string(), plain)))
}

async fn show(State(state): State<AppState>, peer: Option<Extension<ConnectInfo<SocketAddr>>>, headers: HeaderMap, uri: Uri, Path(bridge): Path<String>) -> Response {
    let (org, user) = match member(&state, peer, &headers, &uri, &bridge).await {
        Ok(m) => m,
        Err(response) => return response,
    };
    match load(&state, &org, &user, &bridge).await {
        Ok(found) => Json(json!({ "bridge": bridge, "connected": found.is_some(), "username": found.map(|(u, _)| u) })).into_response(),
        Err(error) => internal(error),
    }
}

#[derive(Deserialize)]
struct Login {
    username: String,
    secret: String,
}

async fn save(State(state): State<AppState>, peer: Option<Extension<ConnectInfo<SocketAddr>>>, headers: HeaderMap, uri: Uri, Path(bridge): Path<String>, Json(login): Json<Login>) -> Response {
    let (org, user) = match member(&state, peer, &headers, &uri, &bridge).await {
        Ok(m) => m,
        Err(response) => return response,
    };
    let username = login.username.trim().to_string();
    if username.is_empty() || username.len() > 320 || login.secret.is_empty() || login.secret.len() > 1000 {
        return failure(StatusCode::UNPROCESSABLE_ENTITY, "give your user name and password");
    }
    // Keep only a login the service accepts.
    let params = json!({ "_user": user, "_credential_override": { "username": username, "secret": login.secret } });
    if let Err(error) = crate::bridges::call(&state, &org, &bridge, "check", &params).await {
        return failure(StatusCode::UNPROCESSABLE_ENTITY, format!("the service did not accept that login: {}", error.message));
    }
    let stored = match state.secrets() {
        Ok(secrets) => secrets.encrypt(&login.secret),
        Err(error) => return internal(error),
    };
    let db = match state.org(&org).await {
        Ok(db) => db,
        Err(error) => return internal(error),
    };
    let result = db
        .query(
            "DELETE user_bridge_credentials WHERE user = $user AND bridge = $bridge; \
             CREATE user_bridge_credentials SET user = $user, bridge = $bridge, username = $username, secret = $secret, updated_at = time::now();",
        )
        .bind(("user", user))
        .bind(("bridge", bridge.clone()))
        .bind(("username", username.clone()))
        .bind(("secret", stored))
        .await
        .and_then(|r| r.check());
    match result {
        Ok(_) => Json(json!({ "bridge": bridge, "connected": true, "username": username })).into_response(),
        Err(error) => internal(error),
    }
}

async fn forget(State(state): State<AppState>, peer: Option<Extension<ConnectInfo<SocketAddr>>>, headers: HeaderMap, uri: Uri, Path(bridge): Path<String>) -> Response {
    let (org, user) = match member(&state, peer, &headers, &uri, &bridge).await {
        Ok(m) => m,
        Err(response) => return response,
    };
    let db = match state.org(&org).await {
        Ok(db) => db,
        Err(error) => return internal(error),
    };
    match db.query("DELETE user_bridge_credentials WHERE user = $user AND bridge = $bridge;").bind(("user", user)).bind(("bridge", bridge.clone())).await.and_then(|r| r.check()) {
        Ok(_) => Json(json!({ "bridge": bridge, "connected": false, "username": Value::Null })).into_response(),
        Err(error) => internal(error),
    }
}
