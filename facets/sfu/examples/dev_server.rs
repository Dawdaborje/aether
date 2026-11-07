//! A bare server for trying the media engine in browsers: a page at `/` and the signaling socket at `/ws`.
//! No sign-in, no plugin: whoever opens `/?room=r&user=ann` is in room `r`. For development and the browser test only.
//!
//!     cargo run -p aether_sfu --example dev_server            (HTTP on 127.0.0.1:17890, media on UDP 127.0.0.1:17891)

use std::net::SocketAddr;

use aether_sfu::{
    engine::{self, Config, Handle, JoinRequest},
    protocol::{ClientFrame, Role, ServerFrame},
    room::Limits,
};
use axum::{
    Router,
    extract::{
        Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::{Html, IntoResponse},
    routing::get,
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;

#[derive(Deserialize)]
struct Who {
    room: String,
    user: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "aether_sfu=info,str0m=warn".into()))
        .init();
    aether_sfu::engine::install_crypto();
    let media: SocketAddr = std::env::var("MEDIA").unwrap_or_else(|_| "127.0.0.1:17891".into()).parse().expect("MEDIA is host:port");
    let http = std::env::var("HTTP").unwrap_or_else(|_| "127.0.0.1:17890".into());
    let handle = engine::spawn(Config { bind: media, advertise: vec![media], limits: Limits::default() }).await.expect("media socket");
    let app = Router::new()
        .route("/", get(|| async { Html(include_str!("../dev/page.html")) }))
        .route("/call-client.js", get(client_script))
        .route("/ws", get(upgrade))
        .route("/bandwidth", get(bandwidth))
        .with_state(handle);
    let listener = tokio::net::TcpListener::bind(&http).await.expect("http port");
    println!("listening on http://{http}  (media udp {media})");
    axum::serve(listener, app).await.unwrap();
}

/// The browser client, made by `dev/browser_test.mjs` from `web/src/lib/call/client.ts` (types stripped).
async fn client_script() -> impl IntoResponse {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/dev/call-client.js");
    match std::fs::read_to_string(path) {
        Ok(code) => ([(axum::http::header::CONTENT_TYPE, "text/javascript")], code).into_response(),
        Err(_) => (axum::http::StatusCode::NOT_FOUND, "run dev/browser_test.mjs, which makes call-client.js").into_response(),
    }
}

#[derive(Deserialize)]
struct Limit {
    room: String,
    user: String,
    /// Bits per second the server should believe the person's connection carries; without it the real estimate returns.
    bps: Option<u64>,
}

async fn bandwidth(State(handle): State<Handle>, Query(limit): Query<Limit>) -> &'static str {
    handle.set_bandwidth(&limit.room, &limit.user, limit.bps).await;
    "ok"
}

async fn upgrade(ws: WebSocketUpgrade, State(handle): State<Handle>, Query(who): Query<Who>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| session(socket, handle, who))
}

async fn session(socket: WebSocket, handle: Handle, who: Who) {
    let (mut sink, mut stream) = socket.split();
    let (out, mut frames) = mpsc::unbounded_channel::<ServerFrame>();
    let joined = handle
        .join(JoinRequest { room: who.room, user: who.user.clone(), name: who.user, role: Role::Member, cap: None, ice: vec![], out })
        .await;
    let conn = match joined {
        Ok(conn) => conn,
        Err(error) => {
            let _ = sink.send(Message::text(serde_json::to_string(&ServerFrame::Error { message: error.to_string() }).unwrap())).await;
            return;
        }
    };
    let writer = tokio::spawn(async move {
        while let Some(frame) = frames.recv().await {
            let removed = matches!(frame, ServerFrame::Removed { .. });
            if sink.send(Message::text(serde_json::to_string(&frame).unwrap())).await.is_err() || removed {
                break;
            }
        }
    });
    while let Some(Ok(message)) = stream.next().await {
        let Message::Text(text) = message else { continue };
        if let Ok(frame) = serde_json::from_str::<ClientFrame>(&text) {
            handle.frame(conn, frame).await;
        }
    }
    handle.leave(conn).await;
    writer.abort();
}
