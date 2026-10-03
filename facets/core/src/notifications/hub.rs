//! Waking connected browsers when a notification is stored.
//!
//! One broadcast channel per organization, created when its first browser connects and
//! removed when the last one leaves, so activity in one organization never wakes
//! another's connections and idle organizations cost nothing.
//!
//! This is the seam for running several servers: [`NotificationHub::publish`] is the one
//! way messages enter, and a pub/sub backend would call it for messages that arrive from
//! other servers (and forward local ones out).

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use serde_json::Value;
use tokio::sync::broadcast;

use super::model::Notification;

const CHANNEL_CAPACITY: usize = 256;

/// A transient event from a plugin (`events::emit`): shown to members now, never stored.
#[derive(Debug, Clone)]
pub struct UiEvent {
    pub plugin: String,
    pub event: String,
    pub payload: Value,
}

#[derive(Debug, Clone)]
pub enum HubMessage {
    Notification(Arc<Notification>),
    Event(Arc<UiEvent>),
}

#[derive(Default)]
struct Inner {
    channels: Mutex<HashMap<String, broadcast::Sender<HubMessage>>>,
    /// Open streams per actor (or per address, for browsers without an identity).
    streams: Mutex<HashMap<String, u32>>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic elsewhere must not take notifications down with it; the maps stay valid.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Clone, Default)]
pub struct NotificationHub {
    inner: Arc<Inner>,
}

impl NotificationHub {
    /// Start receiving an organization's messages.
    pub fn subscribe(&self, org_database: &str) -> broadcast::Receiver<HubMessage> {
        locked(&self.inner.channels)
            .entry(org_database.to_string())
            .or_insert_with(|| broadcast::channel(CHANNEL_CAPACITY).0)
            .subscribe()
    }

    /// Deliver a message to the organization's connected browsers.
    pub fn publish(&self, org_database: &str, message: HubMessage) {
        let mut channels = locked(&self.inner.channels);
        let Some(sender) = channels.get(org_database) else {
            return;
        };
        if sender.send(message).is_err() {
            // Nobody is listening any more.
            channels.remove(org_database);
        }
    }

    /// Reserve one of `key`'s `max` stream slots, or `None` when they are all in use.
    /// The slot is released when the guard is dropped.
    pub fn open_stream(&self, key: &str, max: u32) -> Option<StreamGuard> {
        let mut streams = locked(&self.inner.streams);
        let open = streams.entry(key.to_string()).or_insert(0);
        if *open >= max {
            return None;
        }
        *open += 1;
        Some(StreamGuard {
            hub: self.clone(),
            key: key.to_string(),
        })
    }

    /// End every stream: their channels close, so each connection finishes. Used at
    /// shutdown, so open streams do not hold the server up.
    pub fn close_all(&self) {
        locked(&self.inner.channels).clear();
    }

    /// Open streams across everyone; for logs and tests.
    pub fn open_streams(&self) -> u32 {
        locked(&self.inner.streams).values().sum()
    }
}

/// A reserved stream slot.
pub struct StreamGuard {
    hub: NotificationHub,
    key: String,
}

impl Drop for StreamGuard {
    fn drop(&mut self) {
        let mut streams = locked(&self.hub.inner.streams);
        if let Some(open) = streams.get_mut(&self.key) {
            *open = open.saturating_sub(1);
            if *open == 0 {
                streams.remove(&self.key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notifications::model::{Audience, Level};

    fn notification(id: &str) -> HubMessage {
        HubMessage::Notification(Arc::new(Notification {
            id: id.to_string(),
            source: "kernel".into(),
            level: Level::Info,
            title: "t".into(),
            body: None,
            link: None,
            payload: None,
            created_at: String::new(),
            read: false,
            audience: Audience::Members,
        }))
    }

    #[tokio::test]
    async fn messages_reach_only_their_organization() {
        let hub = NotificationHub::default();
        let mut acme = hub.subscribe("acme");
        let mut globex = hub.subscribe("globex");
        hub.publish("acme", notification("1"));
        assert!(matches!(acme.recv().await, Ok(HubMessage::Notification(n)) if n.id == "1"));
        assert!(globex.try_recv().is_err());
    }

    #[test]
    fn a_channel_without_listeners_is_dropped() {
        let hub = NotificationHub::default();
        drop(hub.subscribe("acme"));
        hub.publish("acme", notification("1"));
        assert!(locked(&hub.inner.channels).is_empty());
        // Publishing to an organization nobody ever listened to is a no-op.
        hub.publish("nobody", notification("2"));
    }

    #[test]
    fn stream_slots_are_limited_and_released() {
        let hub = NotificationHub::default();
        let first = hub.open_stream("users:a", 2);
        let second = hub.open_stream("users:a", 2);
        assert!(first.is_some() && second.is_some());
        assert!(hub.open_stream("users:a", 2).is_none());
        assert!(hub.open_stream("users:b", 2).is_some());
        drop(first);
        assert!(hub.open_stream("users:a", 2).is_some());
        drop(second);
    }
}
