//! The notification endpoints.
//!
//! - `GET  /api/ui/notifications/stream`: server-sent events, one connection per tab.
//! - `GET  /api/ui/notifications`: what the caller may see, newest first.
//! - `POST /api/ui/notifications/read`: mark some (or all) as read.
//!
//! Callers are identified like every other request: members by their session, visitors
//! by their cookie. A browser with no identity yet still gets what is for everyone; it
//! is not given a visitor identity just for listening.

use std::{convert::Infallible, net::SocketAddr, time::Duration};

use axum::{
    Extension, Json, Router,
    extract::{ConnectInfo, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, Uri},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use futures::stream::{self, Stream, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio::{sync::broadcast, time::Instant};

use super::{
    hub::{HubMessage, StreamGuard},
    model::{Notification, Viewer},
    store,
};
use crate::access::{
    audit::Actor,
    identity::{Identity, IdentityError, identify, with_retry_after},
};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/ui/notifications/stream", get(stream_notifications))
        .route("/api/ui/notifications", get(list_notifications))
        .route("/api/ui/notifications/read", post(mark_read))
}

fn failure(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn database_failure(context: &str, error: impl std::fmt::Display) -> Response {
    log::error!("notifications {context}: {error}");
    failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
}

/// Who is asking, from the request's identity.
fn viewer_of(identity: &Identity) -> Viewer {
    match &identity.actor {
        Actor::User(id) => Viewer { actor: Some(id.clone()), member: true },
        Actor::Visitor(id) => Viewer { actor: Some(id.clone()), member: false },
        Actor::Anonymous => Viewer { actor: None, member: false },
    }
}

async fn who(
    state: &AppState,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<Identity, Response> {
    let peer_ip = peer.map(|Extension(ConnectInfo(address))| address.ip());
    identify(state, headers, peer_ip, uri).await.map_err(|error: IdentityError| {
        if matches!(error, IdentityError::Database(_) | IdentityError::Audit(_)) {
            log::error!("notifications identity: {error}");
        }
        let (status, message) = error.http();
        with_retry_after(failure(status, message))
    })
}

// --- the stream ---------------------------------------------------------------------

/// Everything one open stream needs between events.
struct StreamState {
    receiver: broadcast::Receiver<HubMessage>,
    viewer: Viewer,
    /// Missed notifications, sent before anything live.
    backlog: std::vec::IntoIter<Notification>,
    /// The newest notification sent so far; live ones at or below it are duplicates.
    last_sent: Option<String>,
    deadline: Instant,
    closing: bool,
    /// Held so the slot is released when the stream ends.
    _guard: StreamGuard,
}

fn notification_event(notification: &Notification) -> Event {
    Event::default()
        .id(notification.id.clone())
        .event("notification")
        .json_data(notification)
        .unwrap_or_else(|_| Event::default().event("resync"))
}

async fn next_event(mut state: StreamState) -> Option<(Result<Event, Infallible>, StreamState)> {
    if state.closing {
        return None;
    }
    if let Some(notification) = state.backlog.next() {
        state.last_sent = Some(notification.id.clone());
        return Some((Ok(notification_event(&notification)), state));
    }
    loop {
        let message = tokio::select! {
            message = state.receiver.recv() => message,
            () = tokio::time::sleep_until(state.deadline) => {
                // The browser reconnects, which checks its session again.
                state.closing = true;
                return Some((Ok(Event::default().event("reconnect").data("")), state));
            }
        };
        match message {
            Ok(HubMessage::Notification(notification)) => {
                let seen = state.last_sent.as_ref().is_some_and(|last| *last >= notification.id);
                if seen || !notification.audience.includes(&state.viewer) {
                    continue;
                }
                state.last_sent = Some(notification.id.clone());
                return Some((Ok(notification_event(&notification)), state));
            }
            Ok(HubMessage::Event(event)) => {
                if !state.viewer.member {
                    continue;
                }
                let data = json!({
                    "plugin": event.plugin,
                    "event": event.event,
                    "payload": event.payload,
                });
                let frame = Event::default()
                    .event("event")
                    .json_data(&data)
                    .unwrap_or_else(|_| Event::default().event("resync"));
                return Some((Ok(frame), state));
            }
            // Too slow to keep up: tell the browser to fetch what it missed.
            Err(broadcast::error::RecvError::Lagged(_)) => {
                return Some((Ok(Event::default().event("resync").data("")), state));
            }
            Err(broadcast::error::RecvError::Closed) => return None,
        }
    }
}

fn open_stream(state: StreamState) -> impl Stream<Item = Result<Event, Infallible>> {
    stream::unfold(state, next_event)
}

async fn stream_notifications(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let identity = match who(&state, peer, &headers, &uri).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let config = &state.config.notifications;
    let viewer = viewer_of(&identity);

    // Slots are counted per person, or per address for a browser with no identity.
    let key = viewer.actor.clone().unwrap_or_else(|| {
        format!("ip:{}", identity.client_ip.map_or_else(|| "unknown".to_string(), |ip| ip.to_string()))
    });
    let Some(guard) = state.notifications.open_stream(&key, config.max_streams_per_actor) else {
        return failure(StatusCode::TOO_MANY_REQUESTS, "too many open event streams");
    };

    // Listen first, then read what was missed, so nothing falls between the two.
    let receiver = state.notifications.subscribe(&identity.org_db);
    let last_event_id = headers
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let backlog = match last_event_id {
        Some(after) => {
            let db = match state.org(&identity.org_db).await {
                Ok(db) => db,
                Err(error) => return database_failure("stream organization", error),
            };
            match store::after(&db, &viewer, &after, config.replay_limit).await {
                Ok(missed) => missed,
                Err(error) => return database_failure("stream replay", error),
            }
        }
        None => Vec::new(),
    };
    log::info!(
        "{} notification stream opened by {} in `{}` ({} missed)",
        identity.audit.request_id,
        identity.actor.kind(),
        identity.org_db,
        backlog.len()
    );

    let events = open_stream(StreamState {
        receiver,
        viewer,
        last_sent: backlog.last().map(|notification| notification.id.clone()),
        backlog: backlog.into_iter(),
        deadline: Instant::now() + Duration::from_secs(config.stream_lifetime_secs),
        closing: false,
        _guard: guard,
    })
    .boxed();
    let sse = Sse::new(events).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(config.keepalive_secs))
            .text("keep-alive"),
    );
    // Tell reverse proxies not to buffer the stream.
    (
        [("x-accel-buffering", HeaderValue::from_static("no"))],
        sse,
    )
        .into_response()
}

// --- listing and reading ------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ListQuery {
    #[serde(default)]
    unread: bool,
    limit: Option<u32>,
}

async fn list_notifications(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
    Query(query): Query<ListQuery>,
) -> Response {
    let identity = match who(&state, peer, &headers, &uri).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let viewer = viewer_of(&identity);
    let db = match state.org(&identity.org_db).await {
        Ok(db) => db,
        Err(error) => return database_failure("list organization", error),
    };
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let listed = store::list(&db, &viewer, query.unread, limit).await;
    let unread = store::unread_count(&db, &viewer).await;
    match (listed, unread) {
        (Ok(notifications), Ok(unread)) => {
            Json(json!({ "notifications": notifications, "unread": unread })).into_response()
        }
        (Err(error), _) | (_, Err(error)) => database_failure("list", error),
    }
}

#[derive(Debug, Deserialize)]
struct ReadBody {
    /// Notification ids; leave out to mark everything as read.
    ids: Option<Vec<String>>,
}

async fn mark_read(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
    Json(body): Json<ReadBody>,
) -> Response {
    let identity = match who(&state, peer, &headers, &uri).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let viewer = viewer_of(&identity);
    // A browser with no identity has nowhere to record what it read; the page keeps it.
    let Some(actor) = viewer.actor.clone() else {
        return Json(json!({ "marked": 0 })).into_response();
    };
    if body.ids.as_ref().is_some_and(|ids| ids.len() > 500) {
        return failure(StatusCode::BAD_REQUEST, "too many ids");
    }
    let db = match state.org(&identity.org_db).await {
        Ok(db) => db,
        Err(error) => return database_failure("read organization", error),
    };
    match store::mark_read(&db, &viewer, &actor, body.ids).await {
        Ok(marked) => Json(json!({ "marked": marked })).into_response(),
        Err(error) => database_failure("mark read", error),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::notifications::{
        Audience, HubMessage, Level, NotificationHub, UiEvent,
        hub::NotificationHub as Hub,
    };

    fn stored(id: &str, audience: Audience) -> Notification {
        Notification {
            id: id.to_string(),
            source: "kernel".into(),
            level: Level::Info,
            title: format!("title {id}"),
            body: None,
            link: None,
            payload: None,
            created_at: String::new(),
            read: false,
            audience,
        }
    }

    fn state_for(hub: &Hub, viewer: Viewer, backlog: Vec<Notification>, lifetime: Duration) -> StreamState {
        let Some(guard) = hub.open_stream("test", 5) else {
            panic!("the test stream slot is free");
        };
        StreamState {
            receiver: hub.subscribe("acme"),
            viewer,
            last_sent: backlog.last().map(|item| item.id.clone()),
            backlog: backlog.into_iter(),
            deadline: Instant::now() + lifetime,
            closing: false,
            _guard: guard,
        }
    }

    fn member() -> Viewer {
        Viewer { actor: Some("users:a".into()), member: true }
    }

    async fn text(state: StreamState) -> Option<(String, StreamState)> {
        next_event(state).await.map(|(event, state)| (format!("{:?}", event.unwrap_or_else(|never| match never {})), state))
    }

    #[tokio::test]
    async fn the_backlog_comes_first_then_live_messages_without_repeats() {
        let hub = NotificationHub::default();
        let missed = vec![stored("02", Audience::Members)];
        let state = state_for(&hub, member(), missed, Duration::from_secs(60));

        // A live message that overlaps the backlog is dropped; a newer one is sent.
        hub.publish("acme", HubMessage::Notification(Arc::new(stored("02", Audience::Members))));
        hub.publish("acme", HubMessage::Notification(Arc::new(stored("03", Audience::Members))));

        let Some((first, state)) = text(state).await else {
            panic!("backlog item");
        };
        assert!(first.contains("title 02"), "{first}");
        let Some((second, _)) = text(state).await else {
            panic!("live item");
        };
        assert!(second.contains("title 03"), "{second}");
    }

    #[tokio::test]
    async fn only_what_is_for_the_viewer_is_sent() {
        let hub = NotificationHub::default();
        let visitor = Viewer { actor: Some("visitors:v".into()), member: false };
        let state = state_for(&hub, visitor, vec![], Duration::from_secs(60));

        hub.publish("acme", HubMessage::Notification(Arc::new(stored("01", Audience::Members))));
        hub.publish(
            "acme",
            HubMessage::Event(Arc::new(UiEvent { plugin: "chat".into(), event: "typing".into(), payload: json!({}) })),
        );
        hub.publish(
            "acme",
            HubMessage::Notification(Arc::new(stored("02", Audience::Actors { actors: vec!["visitors:other".into()] }))),
        );
        hub.publish("acme", HubMessage::Notification(Arc::new(stored("03", Audience::Everyone))));

        let Some((sent, _)) = text(state).await else {
            panic!("the one for everyone");
        };
        assert!(sent.contains("title 03"), "members-only items, events and others' items are skipped: {sent}");
    }

    #[tokio::test]
    async fn members_receive_plugin_events() {
        let hub = NotificationHub::default();
        let state = state_for(&hub, member(), vec![], Duration::from_secs(60));
        hub.publish(
            "acme",
            HubMessage::Event(Arc::new(UiEvent { plugin: "chat".into(), event: "typing".into(), payload: json!({"who": "ann"}) })),
        );
        let Some((sent, _)) = text(state).await else {
            panic!("event");
        };
        assert!(sent.contains("event: event") || sent.contains("\"event\":\"typing\""), "{sent}");
    }

    #[tokio::test]
    async fn a_viewer_who_fell_behind_is_told_to_resync() {
        let hub = NotificationHub::default();
        let state = state_for(&hub, member(), vec![], Duration::from_secs(60));
        for index in 0..400 {
            hub.publish("acme", HubMessage::Notification(Arc::new(stored(&format!("{index:04}"), Audience::Members))));
        }
        let Some((sent, _)) = text(state).await else {
            panic!("resync");
        };
        assert!(sent.contains("resync"), "{sent}");
    }

    #[tokio::test]
    async fn a_stream_ends_after_its_lifetime_with_a_reconnect_hint() {
        let hub = NotificationHub::default();
        let state = state_for(&hub, member(), vec![], Duration::from_millis(30));
        let Some((sent, state)) = text(state).await else {
            panic!("reconnect hint");
        };
        assert!(sent.contains("reconnect"), "{sent}");
        assert!(next_event(state).await.is_none(), "the stream ends after the hint");
    }

    #[tokio::test]
    async fn closing_the_hub_ends_open_streams_and_frees_their_slots() {
        let hub = NotificationHub::default();
        let state = state_for(&hub, member(), vec![], Duration::from_secs(60));
        assert_eq!(hub.open_streams(), 1);
        hub.close_all();
        assert!(next_event(state).await.is_none());
        assert_eq!(hub.open_streams(), 0, "the slot is released when the stream is dropped");
    }
}
