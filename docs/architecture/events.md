# Events between plugins

A plugin announces something that happened (a chat message was posted, a lead was won, a payment
cleared) and other plugins react, without the first knowing who they are. The same `events::emit` also
updates the browsers of everyone connected, as it always has.

## Announcing

```toml
[plugin]
capabilities = ["events::emit"]

[[events]]
name = "message_posted"      # an emitted event; lower-case letters, digits, _ and -
```

```rhai
events::emit(#{ event: "message_posted", payload: #{ channel: "general", author: "ann" } });
```

The event's **full name** is `<emitting plugin>.<event>`: `chat.message_posted`. Two plugins cannot clash on
a name. Declaring `[[events]]` for what a plugin emits is documentation (it appears with the plugin) and is
checked: an emitted event needs the capability `events::emit`.

## Listening

```toml
[plugin]
dependencies = ["chat"]                  # the emitter must be a dependency
capabilities = ["events::subscribe"]

[[events]]
name = "chat.message_posted"             # <emitting plugin>.<event>
direction = "listen"
handler = "on_message"                   # the function to run
# queue = "default"
# max_attempts = 3
```

```rhai
fn on_message(received) {
    // received = #{ event: "chat.message_posted", source: "chat", payload: #{ ... }, emitted_by: "users:u1", depth: 1 }
    log::info("new message in " + received.payload.channel);
    #{}
}
```

A plugin can also subscribe while it runs, with `events::subscribe("chat.message_posted", "on_message")`
and remove that with `events::unsubscribe("chat.message_posted")` (up to 50 added this way; ones declared in
`plugin.toml` cannot be changed while running). Rust: `events::subscribe`, `events::unsubscribe`,
and `events::Received` as the handler's input.

Listening to another plugin's events is a decision made when the plugin is installed: it must list the
emitter under `dependencies`, and a manifest that listens without that, without `events::subscribe`, or
that names a handler nowhere, is refused when it is loaded. Installing a plugin that depends on `chat` is
the consent to be told what `chat` announces.

## What happens when an event is emitted

1. Connected members of the organization are told (transient; no subscriber needed).
2. Every subscription to `<this plugin>.<event>` in the organization whose plugin is **installed and
   enabled** becomes a **background job** (see [the scheduler](scheduler.md)). Plugins that are not installed
   there, or are disabled, get nothing.
3. Each handler runs as the kernel, under *its own* plugin's capabilities and model grants, never as the
   person whose action caused the event (their id is in `emitted_by`). It is retried if it fails and
   delivered at least once, so write it to be safe to repeat.

The emitting call does not wait for handlers, and a failure to queue one does not undo the emit.

## No endless loops

Every handler job carries `depth`: 1 for an event emitted by an ordinary request, one more for each
handler that emits another event in turn. An event emitted by a handler already **four** deep is shown to
browsers but not passed to plugins (logged as a warning), so two plugins that answer each other's events
cannot run for ever.

## Tables

`event_subscriptions` in every organization database (migration `021_event_subscriptions`), filled from the
catalog's `plugins.event_listeners` (core migration `026_plugin_event_listeners`) when a plugin is installed or
upgraded, plus the ones added while running.
