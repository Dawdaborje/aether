//! Events between plugins.
//!
//! A plugin announces something with `events::emit` (a new chat message, a won lead). Plugins that
//! listen to it have a function run, as a background job, in the same organization. The event's full
//! name is `<emitting plugin>.<event>`, so two plugins cannot clash on a name.
//!
//! Listening is declared in the manifest (`[[events]]` with `direction = "listen"`, which names the
//! handler) and can also be added while running with `events::subscribe`. To listen to a plugin's events
//! a plugin must list that plugin in `dependencies` and hold the capability `events::subscribe`, so
//! installing a plugin that depends on another is the consent to be told what the other announces.
//!
//! A handler is a plugin function that gets
//! `{ event, source, payload, emitted_by, depth }`. It runs as the kernel, under its own plugin's
//! capabilities and model grants, never as the person whose action caused the event, and it is
//! retried like any job; delivery is at least once. An event emitted from inside a handler carries
//! `depth + 1`, and past [`MAX_DEPTH`] it is no longer passed on, so two plugins that answer each
//! other's events cannot loop forever.
//!
//! `events::emit` also pushes the event to the browsers of the organization's connected members, as
//! before (see [`crate::notifications`]); that part is transient and does not need a subscriber.

use serde::Deserialize;
use serde_json::Value;
use surrealdb::{Surreal, engine::remote::ws::Client, types::SurrealValue};

use crate::{
    kernel::host::context::PluginHostContext,
    scheduler::queue::{self, NewJob},
};

/// Events handled in a chain of this many handlers are not passed on any further.
pub const MAX_DEPTH: u32 = 4;
/// Most subscriptions a plugin may add while running.
pub const MAX_RUNTIME_SUBSCRIPTIONS: i64 = 50;

/// `name` is a lower-case word: letters, digits, `_` and `-`.
pub fn is_part(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64 && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// `<plugin>.<event>`, both parts plain.
pub fn is_full_name(name: &str) -> bool {
    name.split_once('.').is_some_and(|(source, event)| is_part(source) && is_part(event))
}

#[derive(Debug, Deserialize)]
struct Listener {
    event: String,
    function: String,
    #[serde(default = "default_queue")]
    queue: String,
    #[serde(default = "default_attempts")]
    max_attempts: i64,
}

fn default_queue() -> String {
    "default".into()
}

fn default_attempts() -> i64 {
    3
}

/// Make the organization's manifest subscriptions for `plugin` match the catalog version's
/// `[[events]]` listeners. Subscriptions the plugin added while running are left alone. Caller must
/// be on the organization's database.
pub async fn sync_listeners(db: &Surreal<Client>, plugin: &str, listeners: Option<&[Value]>) -> Result<(), surrealdb::Error> {
    let defs: Vec<Listener> =
        listeners.unwrap_or_default().iter().filter_map(|value| serde_json::from_value(value.clone()).ok()).collect();
    for def in &defs {
        db.query(
            "UPSERT event_subscriptions SET plugin = $plugin, event = $event, function_name = $function, source = 'manifest', \
             queue = $queue, max_attempts = $attempts, enabled = true WHERE plugin = $plugin AND event = $event;",
        )
        .bind(("plugin", plugin.to_string()))
        .bind(("event", def.event.clone()))
        .bind(("function", def.function.clone()))
        .bind(("queue", def.queue.clone()))
        .bind(("attempts", def.max_attempts))
        .await?
        .check()?;
    }
    let keep: Vec<String> = defs.iter().map(|def| def.event.clone()).collect();
    db.query("DELETE event_subscriptions WHERE plugin = $plugin AND source = 'manifest' AND event NOT IN $keep;")
        .bind(("plugin", plugin.to_string()))
        .bind(("keep", keep))
        .await?
        .check()?;
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum EventError {
    #[error("database error: {0}")]
    Db(#[from] surrealdb::Error),
    #[error("{0}")]
    Refused(String),
}

/// Subscribe the calling plugin to `event` (a full name) from running code.
pub async fn subscribe(ctx: &PluginHostContext, event: &str, function: &str) -> Result<(), EventError> {
    if !is_full_name(event) {
        return Err(EventError::Refused(format!("`{event}` is not an event name; use <plugin>.<event>, such as `chat.message_posted`")));
    }
    if function.is_empty() || function.len() > 128 {
        return Err(EventError::Refused("`function` names one of this plugin's functions".into()));
    }
    let source = event.split_once('.').map_or("", |(source, _)| source);
    if source != ctx.plugin_name && !ctx.dependencies.iter().any(|dependency| dependency == source) {
        return Err(EventError::Refused(format!(
            "`{source}` is not one of this plugin's dependencies; list it under `dependencies` in plugin.toml to listen to its events"
        )));
    }
    let mut response = ctx
        .db
        .query("SELECT VALUE source FROM ONLY event_subscriptions WHERE plugin = $plugin AND event = $event LIMIT 1;")
        .bind(("plugin", ctx.plugin_name.clone()))
        .bind(("event", event.to_string()))
        .await?
        .check()?;
    let existing: Option<String> = response.take(0)?;
    if existing.as_deref() == Some("manifest") {
        return Err(EventError::Refused(format!("`{event}` is declared in plugin.toml and cannot be changed while running")));
    }
    if existing.is_none() {
        let mut response = ctx
            .db
            .query("SELECT count() AS n FROM event_subscriptions WHERE plugin = $plugin AND source = 'runtime' GROUP ALL;")
            .bind(("plugin", ctx.plugin_name.clone()))
            .await?
            .check()?;
        let counted: Option<Value> = response.take(0)?;
        if counted.and_then(|row| row.get("n").and_then(Value::as_i64)).unwrap_or(0) >= MAX_RUNTIME_SUBSCRIPTIONS {
            return Err(EventError::Refused(format!("a plugin may add at most {MAX_RUNTIME_SUBSCRIPTIONS} subscriptions while running")));
        }
    }
    ctx.db
        .query(
            "UPSERT event_subscriptions SET plugin = $plugin, event = $event, function_name = $function, source = 'runtime', \
             queue = 'default', max_attempts = 3, enabled = true WHERE plugin = $plugin AND event = $event;",
        )
        .bind(("plugin", ctx.plugin_name.clone()))
        .bind(("event", event.to_string()))
        .bind(("function", function.to_string()))
        .await?
        .check()?;
    Ok(())
}

/// Remove a subscription added with [`subscribe`]. Returns whether there was one.
pub async fn unsubscribe(ctx: &PluginHostContext, event: &str) -> Result<bool, EventError> {
    let mut response = ctx
        .db
        .query("DELETE event_subscriptions WHERE plugin = $plugin AND event = $event AND source = 'runtime' RETURN BEFORE;")
        .bind(("plugin", ctx.plugin_name.clone()))
        .bind(("event", event.to_string()))
        .await?
        .check()?;
    let removed: Vec<Value> = response.take(0)?;
    Ok(!removed.is_empty())
}

#[derive(Debug, Deserialize, SurrealValue)]
struct Subscriber {
    plugin: String,
    function: String,
    queue: String,
    max_attempts: i64,
}

/// Run the subscribers of `<this plugin>.<event>`: one background job each, for the plugins that are
/// installed and enabled. Returns how many were queued. Does nothing for an event past [`MAX_DEPTH`].
pub async fn dispatch(ctx: &PluginHostContext, event: &str, payload: &Value) -> Result<usize, EventError> {
    if !is_part(event) {
        // A name with dots or capitals is a browser-only event.
        return Ok(0);
    }
    if ctx.event_depth >= MAX_DEPTH {
        log::warn!(
            "{}: event `{}.{event}` is not passed on: handlers have already answered events {} times in a row",
            ctx.database,
            ctx.plugin_name,
            ctx.event_depth
        );
        return Ok(0);
    }
    let full = format!("{}.{event}", ctx.plugin_name);
    let mut response = ctx
        .db
        .query(
            "SELECT plugin, function_name AS function, queue, max_attempts FROM event_subscriptions \
             WHERE event = $event AND enabled = true;",
        )
        .bind(("event", full.clone()))
        .await?
        .check()?;
    let subscribers: Vec<Subscriber> = response.take(0)?;
    if subscribers.is_empty() {
        return Ok(0);
    }
    let mut response = ctx
        .db
        .query("SELECT VALUE plugin_name FROM installed_plugins WHERE is_enabled = true;")
        .await?
        .check()?;
    let installed: Vec<String> = response.take(0)?;
    let mut queued = 0;
    for subscriber in subscribers.into_iter().filter(|s| installed.contains(&s.plugin)) {
        let mut job = NewJob::new("plugin", subscriber.queue);
        job.plugin = Some(subscriber.plugin.clone());
        job.function = Some(subscriber.function);
        job.payload = Some(serde_json::json!({
            "event": full,
            "source": ctx.plugin_name,
            "payload": payload,
            "emitted_by": ctx.audit.actor.id(),
            "depth": ctx.event_depth + 1,
        }));
        job.max_attempts = subscriber.max_attempts;
        job.enqueued_by = Some("system:events".into());
        job.request_id = Some(ctx.audit.request_id.clone());
        match queue::enqueue(&ctx.db, job).await {
            Ok(_) => queued += 1,
            Err(error) => log::warn!("{}: `{full}` could not be passed to `{}`: {error}", ctx.database, subscriber.plugin),
        }
    }
    if queued > 0 {
        if let Some(scheduler) = ctx.services.as_ref().and_then(|services| services.scheduler.clone()) {
            scheduler.wake(&ctx.database);
        }
    }
    Ok(queued)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_names_are_plugin_dot_event() {
        for good in ["chat.message_posted", "crm.lead-won", "a1.b2"] {
            assert!(is_full_name(good), "{good}");
        }
        for bad in ["posted", "chat.", ".posted", "Chat.posted", "chat.a.b", "chat.has space", ""] {
            assert!(!is_full_name(bad), "{bad:?}");
        }
        assert!(is_part("typing") && !is_part("a.b") && !is_part("Typing"));
    }
}
