//! `GET /api/ui/realtime`: the WebSocket (see `docs/architecture/realtime.md`).
//!
//! The upgrade is checked like any request that acts as a person: the session, the organization, and, because a
//! browser sends cookies on a cross-site upgrade, the `Origin` header. Each `sub` is authorized by the plugin that
//! declared the topic, as the person who asks. Frames are JSON text.

use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use axum::{
    Extension, Json, Router,
    extract::{
        ConnectInfo, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::{Value, json};
use tokio::{sync::mpsc, time::Instant};

use super::hub::{Connection, Joined};
use crate::access::{
    audit::Actor,
    identity::{Identity, IdentityError, identify, with_retry_after},
};
use crate::plugin_manager::models::plugin_def::PluginRealtimeDef;
use crate::state::AppState;

const MAX_FRAME: usize = 16 * 1024;

/// The kernel's own presence topic: everyone with a signed-in page in the organization joins it (the app shell does,
/// for the whole session), so "online" does not depend on which app the person has open. A page that is looking at
/// an app that shows people is *active*, every other page is *idle*. No plugin declares or publishes to it.
pub const ONLINE: &str = "kernel:online";

fn online_def() -> PluginRealtimeDef {
    PluginRealtimeDef {
        topic: "online".to_string(),
        subscribe: String::new(),
        presence: true,
        client_events: Vec::new(),
        history: 0,
    }
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/ui/realtime", get(upgrade))
}

fn failure(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

/// What the settings say about this organization.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_connections: u32,
    pub max_topics: usize,
    pub client_rate: u32,
    pub idle_secs: u64,
}

async fn limits(state: &AppState, org_db: &str) -> Limits {
    async fn number(state: &AppState, org_db: &str, key: &str, default: u64) -> u64 {
        crate::application::settings::get_setting(state, key, Some(&crate::chatter::org_ref(org_db)))
            .await
            .ok()
            .flatten()
            .and_then(|setting| setting.value.as_u64())
            .filter(|n| *n > 0)
            .unwrap_or(default)
    }
    Limits {
        max_connections: number(state, org_db, "realtime.max_connections", 8).await.min(1000) as u32,
        max_topics: number(state, org_db, "realtime.max_topics", 100).await.min(10_000) as usize,
        client_rate: number(state, org_db, "realtime.client_rate", 10).await.min(1000) as u32,
        idle_secs: number(state, org_db, "realtime.idle_secs", 60).await.clamp(10, 3600),
    }
}

/// Whether the `Origin` of an upgrade is this site or a trusted origin. A missing origin is refused: browsers always
/// send one on a WebSocket, so a request without it is not a page of ours.
pub fn origin_ok(headers: &HeaderMap, peer: Option<IpAddr>, trusted_proxies: &[IpAddr], trusted_origins: &[String]) -> bool {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    if trusted_origins.iter().any(|allowed| allowed == origin) {
        return true;
    }
    let via_proxy = peer.is_some_and(|ip| trusted_proxies.contains(&ip));
    let host = if via_proxy {
        headers.get("x-forwarded-host").or_else(|| headers.get(header::HOST))
    } else {
        headers.get(header::HOST)
    }
    .and_then(|v| v.to_str().ok())
    .and_then(|v| v.split(',').next())
    .map(str::trim)
    .unwrap_or_default();
    let Some((_, authority)) = origin.split_once("://") else {
        return false;
    };
    !host.is_empty() && authority.eq_ignore_ascii_case(host)
}

async fn upgrade(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
    ws: WebSocketUpgrade,
) -> Response {
    let peer_ip = peer.map(|Extension(ConnectInfo(address))| address.ip());
    let server = state.config.server.as_ref();
    let proxies = server.map(|s| s.trusted_proxies.as_slice()).unwrap_or_default();
    let origins = server.map(|s| s.csrf_trusted_origins.as_slice()).unwrap_or_default();
    if !origin_ok(&headers, peer_ip, proxies, origins) {
        return failure(StatusCode::FORBIDDEN, "cross-site connection refused");
    }
    let identity = match identify(&state, &headers, peer_ip, &uri).await {
        Ok(identity) => identity,
        Err(error) => {
            if matches!(error, IdentityError::Database(_) | IdentityError::Audit(_)) {
                log::error!("realtime identity: {error}");
            }
            let (status, message) = error.http();
            return with_retry_after(failure(status, message));
        }
    };
    let Actor::User(user) = identity.actor.clone() else {
        return failure(StatusCode::UNAUTHORIZED, "sign in to connect");
    };
    let limits = limits(&state, &identity.org_db).await;
    let hub = state.notifications.realtime().clone();
    let Some((conn, rx)) = hub.connect(&identity.org_db, &user, limits.max_connections) else {
        return failure(StatusCode::TOO_MANY_REQUESTS, "too many open connections");
    };
    ws.max_message_size(MAX_FRAME)
        .max_frame_size(MAX_FRAME)
        .on_upgrade(move |socket| run(socket, state, identity, conn, rx, limits))
}

/// A topic this connection joined, with what its plugin declared.
struct Joined_ {
    def: PluginRealtimeDef,
}

/// The plugin, the part after the prefix, the declaration it fits and its `{name}` values.
async fn declared(
    state: &AppState,
    org_db: &str,
    full: &str,
) -> Option<(String, String, PluginRealtimeDef, serde_json::Map<String, Value>)> {
    let (plugin, rest) = full.split_once(':')?;
    if plugin.is_empty() || rest.is_empty() || full.len() > 200 {
        return None;
    }
    let org = state.org(org_db).await.ok()?;
    let core = state.core().await.ok()?;
    let installed = crate::plugin_manager::installed::list_cached(&state.cache, org_db, &org).await.ok()?;
    let row = installed.into_iter().find(|row| row.plugin_name == plugin && row.is_enabled)?;
    let loaded = state.plugin_runtime.ensure_loaded(&core, plugin, &row.version).await.ok()?;
    loaded.manifest.realtime.iter().find_map(|def| {
        def.match_topic(rest).map(|params| (plugin.to_string(), rest.to_string(), def.clone(), params))
    })
}

/// Ask the plugin whether this person may join. `Err` carries the reason shown to the browser.
async fn authorize(
    state: &AppState,
    identity: &mut Identity,
    plugin: &str,
    rest: &str,
    def: &PluginRealtimeDef,
    params: &serde_json::Map<String, Value>,
) -> Result<(), String> {
    let Actor::User(user) = identity.actor.clone() else {
        return Err("sign in to join".into());
    };
    let payload = json!({ "topic": rest, "params": params, "user": user });
    match crate::plugin_manager::api::run_as_caller(state, identity, plugin, &def.subscribe, payload).await {
        Ok(answer) if answer.get("allow").and_then(Value::as_bool) == Some(true) => Ok(()),
        Ok(answer) => Err(answer.get("reason").and_then(Value::as_str).unwrap_or("not allowed").chars().take(200).collect()),
        Err((403 | 404, _)) => Err("not allowed".into()),
        Err(_) => Err("unavailable".into()),
    }
}

async fn send(socket: &mut WebSocket, frame: &Value) -> bool {
    socket.send(Message::Text(frame.to_string().into())).await.is_ok()
}

/// Events per second a connection may send, as a counter over one-second windows.
struct Rate {
    started: Instant,
    count: u32,
    max: u32,
}

impl Rate {
    fn allow(&mut self) -> bool {
        if self.started.elapsed() >= Duration::from_secs(1) {
            self.started = Instant::now();
            self.count = 0;
        }
        self.count += 1;
        self.count <= self.max
    }
}

async fn run(mut socket: WebSocket, state: AppState, mut identity: Identity, conn: Connection, mut rx: mpsc::Receiver<Value>, limits: Limits) {
    let hub = state.notifications.realtime().clone();
    let mut joined: HashMap<String, Joined_> = HashMap::new();
    let mut rate = Rate { started: Instant::now(), count: 0, max: limits.client_rate };
    let lifetime = Duration::from_secs(state.config.notifications.stream_lifetime_secs);
    let ends = Instant::now() + lifetime;
    let mut last_heard = Instant::now();
    let mut tick = tokio::time::interval(Duration::from_secs((limits.idle_secs / 3).max(5)));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut dropped_events = 0u64;

    let reason = loop {
        tokio::select! {
            incoming = socket.recv() => {
                let Some(Ok(message)) = incoming else { break None };
                last_heard = Instant::now();
                let text = match message {
                    Message::Text(text) => text,
                    Message::Close(_) => break None,
                    Message::Ping(_) | Message::Pong(_) => continue,
                    Message::Binary(_) => { break Some("binary frames are not used") }
                };
                let Ok(frame) = serde_json::from_str::<Value>(&text) else {
                    if !send(&mut socket, &json!({ "t": "error", "reason": "frames are JSON" })).await { break None }
                    continue;
                };
                let topic = frame.get("topic").and_then(Value::as_str).unwrap_or_default().to_string();
                match frame.get("op").and_then(Value::as_str).unwrap_or_default() {
                    "ping" => { if !send(&mut socket, &json!({ "t": "pong" })).await { break None } }
                    "sub" => {
                        let since = frame.get("since").and_then(Value::as_u64);
                        if !joined.contains_key(&topic) && joined.len() >= limits.max_topics {
                            let denied = json!({ "t": "sub", "topic": topic, "ok": false, "reason": "too many topics" });
                            if !send(&mut socket, &denied).await { break None }
                            continue;
                        }
                        if topic == ONLINE {
                            // Open to every signed-in person of the organization; there is no plugin to ask.
                            if hub.join(conn.id, &topic, 0, true, since) == Joined::Gone { break Some("too slow") }
                            joined.insert(topic, Joined_ { def: online_def() });
                            continue;
                        }
                        let Some((plugin, rest, def, params)) = declared(&state, &identity.org_db, &topic).await else {
                            let denied = json!({ "t": "sub", "topic": topic, "ok": false, "reason": "unknown topic" });
                            if !send(&mut socket, &denied).await { break None }
                            continue;
                        };
                        if let Err(why) = authorize(&state, &mut identity, &plugin, &rest, &def, &params).await {
                            let denied = json!({ "t": "sub", "topic": topic, "ok": false, "reason": why });
                            if !send(&mut socket, &denied).await { break None }
                            continue;
                        }
                        if hub.join(conn.id, &topic, def.history, def.presence, since) == Joined::Gone { break Some("too slow") }
                        joined.insert(topic, Joined_ { def });
                    }
                    "unsub" => {
                        hub.leave(conn.id, &topic);
                        joined.remove(&topic);
                    }
                    "idle" => {
                        // The person is on the topic but not looking at it (another page or app, a hidden tab).
                        if joined.contains_key(&topic) {
                            hub.set_idle(conn.id, &topic, frame.get("idle").and_then(Value::as_bool).unwrap_or(true));
                        }
                    }
                    "event" => {
                        let kind = frame.get("kind").and_then(Value::as_str).unwrap_or_default();
                        let allowed = joined.get(&topic).is_some_and(|j| j.def.client_events.iter().any(|k| k == kind));
                        if !allowed || !rate.allow() {
                            dropped_events += 1;
                            continue;
                        }
                        let payload = frame.get("payload").cloned().unwrap_or(Value::Null);
                        if payload.to_string().len() > 2048 { dropped_events += 1; continue }
                        hub.send_event(conn.id, &topic, kind, payload);
                    }
                    _ => { if !send(&mut socket, &json!({ "t": "error", "reason": "unknown op" })).await { break None } }
                }
            }
            outgoing = rx.recv() => {
                match outgoing {
                    Some(frame) => {
                        // A topic the plugin ended for this person is forgotten here too.
                        if frame.get("t").and_then(Value::as_str) == Some("dropped")
                            && let Some(topic) = frame.get("topic").and_then(Value::as_str)
                        {
                            joined.remove(topic);
                        }
                        if !send(&mut socket, &frame).await { break None }
                    }
                    None => break Some("closed"),
                }
            }
            _ = tick.tick() => {
                if Instant::now() >= ends { break Some("reconnect") }
                if last_heard.elapsed() > Duration::from_secs(limits.idle_secs) { break Some("idle") }
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() { break None }
            }
        }
    };
    if dropped_events > 0 {
        log::debug!("realtime: {dropped_events} client events refused on connection {}", conn.id);
    }
    if let Some(reason) = reason {
        let _ = send(&mut socket, &json!({ "t": "bye", "reason": reason })).await;
    }
    let _ = socket.send(Message::Close(None)).await;
    // `conn` drops here and leaves every topic.
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(origin: Option<&str>, host: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, HeaderValue::from_str(host).unwrap());
        if let Some(origin) = origin {
            h.insert(header::ORIGIN, HeaderValue::from_str(origin).unwrap());
        }
        h
    }

    #[test]
    fn only_this_site_or_a_trusted_origin_may_connect() {
        let none: &[String] = &[];
        assert!(origin_ok(&headers(Some("https://app.example"), "app.example"), None, &[], none));
        assert!(!origin_ok(&headers(Some("https://evil.example"), "app.example"), None, &[], none));
        assert!(!origin_ok(&headers(None, "app.example"), None, &[], none), "no origin is refused");
        assert!(!origin_ok(&headers(Some("null"), "app.example"), None, &[], none));
        assert!(origin_ok(
            &headers(Some("https://other.example"), "app.example"),
            None,
            &[],
            &["https://other.example".to_string()]
        ));
        // Forwarded host only counts from a trusted proxy.
        let mut forwarded = headers(Some("https://public.example"), "internal:8080");
        forwarded.insert("x-forwarded-host", HeaderValue::from_static("public.example"));
        let proxy: IpAddr = "10.0.0.1".parse().unwrap();
        assert!(origin_ok(&forwarded, Some(proxy), &[proxy], none));
        assert!(!origin_ok(&forwarded, Some("203.0.113.9".parse().unwrap()), &[proxy], none));
    }

    #[test]
    fn the_event_rate_is_limited_per_second() {
        let mut rate = Rate { started: Instant::now(), count: 0, max: 3 };
        assert!((0..3).all(|_| rate.allow()));
        assert!(!rate.allow());
    }
}
