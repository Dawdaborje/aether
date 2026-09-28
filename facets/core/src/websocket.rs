use std::sync::Arc;

use axum::{
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode},
    response::Response,
};
use axum_extra::extract::CookieJar;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast;

use crate::state::AppState;

const SESSION_COOKIE: &str = "aether_session";
const NOTIFICATION_CHANNEL_CAPACITY: usize = 256;

#[derive(Debug, Clone, Serialize)]
pub struct UiNotification {
    pub plugin: String,
    pub event: String,
    pub payload: Value,
}

#[derive(Debug, Clone)]
pub struct ScopedNotification {
    pub org_database: String,
    pub notification: UiNotification,
}

#[derive(Clone)]
pub struct NotificationHub {
    sender: Arc<broadcast::Sender<ScopedNotification>>,
}

impl Default for NotificationHub {
    fn default() -> Self {
        let (sender, _) = broadcast::channel(NOTIFICATION_CHANNEL_CAPACITY);
        Self {
            sender: Arc::new(sender),
        }
    }
}

impl NotificationHub {
    pub fn subscribe(&self) -> broadcast::Receiver<ScopedNotification> {
        self.sender.subscribe()
    }

    pub fn publish(&self, org_database: String, notification: UiNotification) {
        let _ = self.sender.send(ScopedNotification {
            org_database,
            notification,
        });
    }
}

pub async fn notifications_ws(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, (StatusCode, &'static str)> {
    if !valid_origin(&headers) {
        return Err((StatusCode::FORBIDDEN, "websocket origin not allowed"));
    }

    let token = jar
        .get(SESSION_COOKIE)
        .map(|cookie| cookie.value())
        .filter(|token| !token.is_empty())
        .ok_or((StatusCode::UNAUTHORIZED, "not authenticated"))?;

    state.use_core().await.map_err(|err| {
        log::error!("notification websocket database selection failed: {err}");
        (StatusCode::INTERNAL_SERVER_ERROR, "database error")
    })?;

    let session = aether_orm::find_session_by_token(&state.db, token)
        .await
        .map_err(|err| {
            log::error!("notification websocket session lookup failed: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "database error")
        })?
        .filter(|session| session.user.is_active)
        .ok_or((StatusCode::UNAUTHORIZED, "invalid session"))?;

    let org_database = session
        .org_database_id
        .unwrap_or_else(|| state.core_database.clone());
    let receiver = state.notifications.subscribe();

    Ok(ws.on_upgrade(move |socket| serve_notifications(socket, receiver, org_database)))
}

async fn serve_notifications(
    mut socket: WebSocket,
    mut receiver: broadcast::Receiver<ScopedNotification>,
    org_database: String,
) {
    loop {
        tokio::select! {
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(Message::Ping(payload))) => {
                        if socket.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(_)) => {}
                }
            }
            notification = receiver.recv() => {
                match notification {
                    Ok(message) if message.org_database == org_database => {
                        let frame = serde_json::json!({
                            "type": "plugin.notification",
                            "plugin": message.notification.plugin,
                            "event": message.notification.event,
                            "payload": message.notification.payload,
                        });
                        if socket.send(Message::Text(frame.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

fn valid_origin(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(axum::http::header::ORIGIN) else {
        return true;
    };
    let Some(host) = headers
        .get(axum::http::header::HOST)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    let Some(authority) = origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"))
    else {
        return false;
    };
    let authority = authority.split('/').next().unwrap_or_default();
    !authority.is_empty() && authority.eq_ignore_ascii_case(host)
}
