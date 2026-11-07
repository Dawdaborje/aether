//! `GET /api/ui/sfu/{room}`: the signaling socket of a video meeting (see `docs/architecture/meet_sfu.md`).
//!
//! `{room}` is `<plugin>:<room as the plugin names it>`. The upgrade is checked like the [realtime](crate::realtime)
//! socket: the session, the organization and the `Origin`. The plugin that declared the room then decides, on every
//! join and as the person who asks, whether they may come in and as what. The media itself never touches this socket;
//! it flows over the engine's UDP port.

use std::net::{IpAddr, SocketAddr};

use aether_sfu::{
    engine::{self, Handle, JoinRequest},
    protocol::{ClientFrame, Role, ServerFrame},
    room::{self, Limits},
    turn,
};
use axum::{
    Extension, Json, Router,
    extract::{
        ConnectInfo, Path, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::{Value, json};
use tokio::{sync::mpsc, time::Instant};

use crate::access::{
    audit::Actor,
    identity::{IdentityError, identify, with_retry_after},
};
use crate::config_manager::models::SfuConfig;
use crate::plugin_manager::models::plugin_def::PluginSfuDef;
use crate::realtime::ws::origin_ok;
use crate::state::AppState;

/// A signaling frame carries an SDP, which is a few kilobytes; nothing legitimate is larger.
const MAX_FRAME: usize = 64 * 1024;
/// Frames a person may send per second (candidates arrive in bursts).
const MAX_RATE: u32 = 60;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/ui/sfu/{room}", get(upgrade))
}

fn failure(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

/// What the media server runs with: the `[sfu]` section of aether.toml, overridden by the Meetings settings where one is
/// set. The port and whether the server runs at all stay with the file; everything else can be changed from the desk
/// and takes effect for the next person to join.
struct Effective {
    public_ips: Vec<IpAddr>,
    turn_urls: Vec<String>,
    turn_secret: Option<String>,
    credential_ttl_secs: u64,
    limits: Limits,
    /// This organization's own, lower, limit for people in one meeting.
    org_cap: Option<usize>,
    /// Meetings switched on (globally and for this organization).
    on: bool,
}

async fn effective(state: &AppState, org_db: &str, file: &SfuConfig) -> Effective {
    use crate::application::settings::{Scope, get_setting_plain, get_setting_plain_in};
    let org = crate::chatter::org_ref(org_db);
    let global = |key: &'static str| async move {
        get_setting_plain_in(state, key, Scope::Global).await.ok().flatten()
    };
    let number = |value: Option<Value>| value.and_then(|v| v.as_u64()).filter(|n| *n > 0);
    let text = |value: Option<Value>| value.and_then(|v| v.as_str().map(|t| t.trim().to_string())).filter(|t| !t.is_empty());
    let list = |value: Option<Value>| -> Vec<String> {
        text(value)
            .map(|t| t.split([',', '\n', ' ']).map(str::trim).filter(|p| !p.is_empty()).map(str::to_string).collect())
            .unwrap_or_default()
    };
    let public_ips: Vec<IpAddr> = list(global("meetings.public_ips").await)
        .iter()
        .filter_map(|ip| match ip.parse() {
            Ok(ip) => Some(ip),
            Err(_) => {
                log::warn!("sfu: `meetings.public_ips` has `{ip}`, which is not an IP address; ignored");
                None
            }
        })
        .collect();
    let turn_urls = list(global("meetings.turn_urls").await);
    let on = |value: Option<Value>| value.and_then(|v| v.as_bool()).unwrap_or(true);
    let globally_on = on(global("meetings.enabled").await);
    let org_on = on(get_setting_plain(state, "meetings.enabled", Some(&org)).await.ok().flatten());
    Effective {
        public_ips: if public_ips.is_empty() { file.public_ips.clone() } else { public_ips },
        turn_urls: if turn_urls.is_empty() { file.turn_urls.clone() } else { turn_urls },
        turn_secret: text(global("meetings.turn_secret").await).or_else(|| file.turn_secret.clone()),
        credential_ttl_secs: number(global("meetings.credential_ttl_secs").await).unwrap_or(file.credential_ttl_secs),
        limits: Limits {
            max_rooms: number(global("meetings.max_rooms").await).map_or(file.max_rooms, |n| n as usize),
            max_participants: number(global("meetings.max_participants").await).map_or(file.max_participants, |n| n as usize),
        },
        org_cap: number(get_setting_plain(state, "meetings.max_participants", Some(&org)).await.ok().flatten()).map(|n| n as usize),
        on: globally_on && org_on,
    }
}

/// Where browsers are told to send media: the configured addresses with the media port, else the bind address.
fn advertised(public_ips: &[IpAddr], bind: SocketAddr) -> Vec<SocketAddr> {
    if !public_ips.is_empty() {
        return public_ips.iter().map(|ip| SocketAddr::new(*ip, bind.port())).collect();
    }
    if bind.ip().is_unspecified() {
        log::warn!("sfu: `public_ips` is empty, so only browsers on this machine can reach the media port; set it in [sfu]");
        return vec![SocketAddr::new(IpAddr::from([127, 0, 0, 1]), bind.port())];
    }
    vec![bind]
}

/// The engine, started on first use. A bad `[sfu]` section is reported to whoever tried to join.
async fn engine_for(state: &AppState, config: &SfuConfig, live: &Effective) -> Result<Handle, String> {
    let handle = state
        .notifications
        .sfu()
        .get_or_try_init(|| async {
            let bind: SocketAddr = config.bind.parse().map_err(|_| format!("`sfu.bind` is not an address: {}", config.bind))?;
            aws_provider();
            engine::spawn(engine::Config {
                bind,
                advertise: advertised(&live.public_ips, bind),
                limits: live.limits,
            })
            .await
            .map_err(|error| format!("the media port {bind} could not be opened: {error}"))
        })
        .await
        .map_err(|error: String| {
            log::error!("sfu: {error}");
            "meetings are unavailable on this server".to_string()
        })?;
    // Settings changed since the engine last heard: tell it, so a new address or limit applies without a restart.
    let bind: SocketAddr = config.bind.parse().map_err(|_| "meetings are unavailable on this server".to_string())?;
    let wanted = (advertised(&live.public_ips, bind), live.limits);
    static APPLIED: std::sync::Mutex<Option<(Vec<SocketAddr>, Limits)>> = std::sync::Mutex::new(None);
    let changed = {
        let mut applied = APPLIED.lock().unwrap_or_else(|e| e.into_inner());
        let changed = applied.as_ref().is_some_and(|last| *last != wanted);
        *applied = Some(wanted.clone());
        changed
    };
    if changed {
        handle.configure(wanted.0, wanted.1).await;
    }
    Ok(handle.clone())
}

/// The WebRTC library needs its crypto provider installed once per process.
fn aws_provider() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| aether_sfu::engine::install_crypto());
}

/// The room a plugin declared that fits `rest`, with its `{name}` values.
async fn declared(
    state: &AppState,
    org_db: &str,
    plugin: &str,
    rest: &str,
) -> Option<(PluginSfuDef, serde_json::Map<String, Value>)> {
    if plugin.is_empty() || rest.is_empty() || rest.len() > 200 {
        return None;
    }
    let org = state.org(org_db).await.ok()?;
    let core = state.core().await.ok()?;
    let installed = crate::plugin_manager::installed::list_cached(&state.cache, org_db, &org).await.ok()?;
    let row = installed.into_iter().find(|row| row.plugin_name == plugin && row.is_enabled)?;
    let loaded = state.plugin_runtime.ensure_loaded(&core, plugin, &row.version).await.ok()?;
    loaded.manifest.sfu.iter().find_map(|def| def.match_room(rest).map(|params| (def.clone(), params)))
}

/// Said inside the socket, so the page can show the reason: a refused upgrade tells a browser nothing but "failed".
fn refuse(ws: WebSocketUpgrade, message: &'static str) -> Response {
    ws.on_upgrade(move |mut socket| async move {
        let _ = send(&mut socket, &ServerFrame::Error { message: message.into() }).await;
        let _ = socket.send(Message::Close(None)).await;
    })
}

async fn upgrade(
    State(state): State<AppState>,
    Path(full): Path<String>,
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
    let config = state.config.sfu.clone();
    if !config.enabled {
        return refuse(ws, "Video meetings are not switched on for this server. An administrator turns them on with `[sfu] enabled = true` in aether.toml.");
    }
    let mut identity = match identify(&state, &headers, peer_ip, &uri).await {
        Ok(identity) => identity,
        Err(error) => {
            if matches!(error, IdentityError::Database(_) | IdentityError::Audit(_)) {
                log::error!("sfu identity: {error}");
            }
            let (status, message) = error.http();
            return with_retry_after(failure(status, message));
        }
    };
    let Actor::User(user) = identity.actor.clone() else {
        return failure(StatusCode::UNAUTHORIZED, "sign in to join");
    };
    let Some((plugin, rest)) = full.split_once(':') else {
        return failure(StatusCode::NOT_FOUND, "unknown meeting");
    };
    let (plugin, rest) = (plugin.to_string(), rest.to_string());
    let live = effective(&state, &identity.org_db, &config).await;
    if !live.on {
        return refuse(ws, "Video meetings are switched off for this organization. An administrator turns them on in Settings, under Meetings.");
    }
    let Some((def, params)) = declared(&state, &identity.org_db, &plugin, &rest).await else {
        return failure(StatusCode::NOT_FOUND, "unknown meeting");
    };
    // The plugin decides, every time: someone removed from the meeting cannot come back in.
    let payload = json!({ "room": rest, "params": params, "user": user });
    let answer = match crate::plugin_manager::api::run_as_caller(&state, &mut identity, &plugin, &def.join, payload).await {
        Ok(answer) if answer.get("allow").and_then(Value::as_bool) == Some(true) => answer,
        Ok(answer) => {
            let reason: String = answer.get("reason").and_then(Value::as_str).unwrap_or("not allowed").chars().take(200).collect();
            return failure(StatusCode::FORBIDDEN, &reason);
        }
        Err((403 | 404, _)) => return failure(StatusCode::FORBIDDEN, "not allowed"),
        Err(_) => return failure(StatusCode::BAD_GATEWAY, "unavailable"),
    };
    let role = match answer.get("role").and_then(Value::as_str) {
        Some("host") => Role::Host,
        _ => Role::Member,
    };
    let name = answer.get("name").and_then(Value::as_str).map(|n| n.chars().take(120).collect::<String>()).filter(|n| !n.trim().is_empty());
    let engine = match engine_for(&state, &config, &live).await {
        Ok(engine) => engine,
        Err(message) => return failure(StatusCode::SERVICE_UNAVAILABLE, &message),
    };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let ice = turn::ice_servers(&live.turn_urls, live.turn_secret.as_deref(), &user, now, live.credential_ttl_secs);
    let request = Seat {
        engine,
        room: room::key(&identity.org_db, &plugin, &rest),
        user: user.clone(),
        name: name.unwrap_or(user),
        role,
        cap: match (def.max_participants, live.org_cap) {
            (Some(plugin), Some(org)) => Some(plugin.min(org)),
            (plugin, org) => plugin.or(org),
        },
        ice,
    };
    ws.max_message_size(MAX_FRAME).max_frame_size(MAX_FRAME).on_upgrade(move |socket| run(socket, request))
}

/// Everything decided before the socket opens.
struct Seat {
    engine: Handle,
    room: String,
    user: String,
    name: String,
    role: Role,
    cap: Option<usize>,
    ice: Vec<aether_sfu::protocol::IceServer>,
}

async fn send(socket: &mut WebSocket, frame: &ServerFrame) -> bool {
    match serde_json::to_string(frame) {
        Ok(text) => socket.send(Message::Text(text.into())).await.is_ok(),
        Err(_) => false,
    }
}

async fn run(mut socket: WebSocket, seat: Seat) {
    let (out, mut frames) = mpsc::unbounded_channel::<ServerFrame>();
    let conn = match seat
        .engine
        .join(JoinRequest { room: seat.room, user: seat.user, name: seat.name, role: seat.role, cap: seat.cap, ice: seat.ice, out })
        .await
    {
        Ok(conn) => conn,
        Err(error) => {
            let _ = send(&mut socket, &ServerFrame::Error { message: error.to_string() }).await;
            return;
        }
    };
    let mut window = (Instant::now(), 0u32);
    loop {
        tokio::select! {
            frame = frames.recv() => {
                // The engine dropped this person (removed, or the room ended): say why, then close.
                let Some(frame) = frame else { break };
                if !send(&mut socket, &frame).await { break }
                if matches!(frame, ServerFrame::Removed { .. }) { break }
            }
            incoming = socket.recv() => {
                let Some(Ok(message)) = incoming else { break };
                match message {
                    Message::Text(text) => {
                        if window.0.elapsed().as_secs() >= 1 { window = (Instant::now(), 0) }
                        window.1 += 1;
                        if window.1 > MAX_RATE { continue }
                        match serde_json::from_str::<ClientFrame>(&text) {
                            Ok(ClientFrame::Leave) => break,
                            Ok(frame) => seat.engine.frame(conn, frame).await,
                            Err(_) => {
                                if !send(&mut socket, &ServerFrame::Error { message: "that frame is not understood".into() }).await { break }
                            }
                        }
                    }
                    Message::Close(_) => break,
                    Message::Binary(_) => break,
                    Message::Ping(_) | Message::Pong(_) => {}
                }
            }
        }
    }
    seat.engine.leave(conn).await;
}
