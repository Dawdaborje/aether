---
title: "Realtime: WebSocket, topics and presence"
description: "A two-way channel between the browser and the kernel for chat-like apps: topics a plugin declares and authorizes, live messages, who is online, typing, and resuming after a dropped connection."
section: architecture
status: stable
order: 220
---
# Realtime: WebSocket, topics and presence

**Status: built, apart from what is listed under "Not built".** Built: the hub (`facets/core/src/realtime/hub.rs`), the socket (`ws.rs`, `GET /api/ui/realtime`), `[[realtime]]` in `plugin.toml`, the `realtime::publish`, `realtime::drop` and `realtime::presence` commands, `?org=` for header tenancy, the settings `realtime.*`, the Rust SDK module `realtime`, and the browser client (`web/src/lib/realtime/`). It is checked against a running server with a real plugin: `tests/e2e/realtime` (37 checks over a real WebSocket, and 22 in a browser for the chat page). **Not built:** the pub/sub seam for several servers, a TypeScript/Go/Java SDK module, and a desk page listing open connections. Choices are marked *Decide* in the last section.

[Notifications](notifications.md) push to every member of an organization over server-sent events: one way, no
topics, no presence. That is right for "a lead was won". It cannot do a chat: a private channel must reach only its
members, the browser must tell the server it is typing, and everyone must see who is online. This page adds a
**WebSocket** next to the event stream, with **topics** that plugins declare and authorize. The event stream stays.

A WebSocket carries a **signal and a small payload**, never the record of truth. A message is stored by the plugin in
its own model first (audited, with history, searchable); the realtime channel then tells the others "message 41 is in
channel 7". A client that was offline reads the missed ones from the database, so nothing depends on delivery.

## What a plugin declares

```toml
[[realtime]]
topic = "channel.{channel}"          # a pattern; {name} matches one segment
subscribe = "can_join_channel"       # function called before anyone joins; returns { "allow": true }
presence = true                      # track who is connected to this topic
client_events = ["typing"]           # kinds a browser may send to the others on this topic
history = 200                        # how many recent messages to keep for resuming (default 100, 0 = none)
```

The full topic on the wire is `<plugin>:<topic>`, for example `chat:channel.7`. A plugin can only declare and publish
topics in its own namespace, so one plugin cannot listen to another's channels.

`subscribe` is a function of the same plugin. The kernel calls it **as the person who asks**, with `{ topic, params,
user }` (`params` are the `{name}` parts), under the person's roles and record rules. It answers `{ "allow": true }`
or refuses; it may also return `{ "allow": true, "label": "…" }` to name the topic in presence lists. The answer is not
cached across reconnects, so removing someone from a channel stops their next join. When membership changes, the
plugin calls `realtime::drop` to end the subscriptions of a person at once.

## Publishing

New capability group `realtime::`:

| Command | Effect |
|---|---|
| `realtime::publish { topic, event, payload, except_user? }` | Sends to everyone subscribed to the topic. Payload at most 16 KB. The topic must match a pattern the plugin declared. |
| `realtime::drop { topic, user }` | Ends one person's subscription (they left the channel, or were removed). |
| `realtime::presence { topic }` | Who is connected now: `[{ user, since }]`. |

A published message gets the next **sequence number of its topic**; the kernel keeps the last `history` messages in memory.

## The socket

`GET /api/ui/realtime`, upgraded to a WebSocket. It authenticates like the event stream (the session cookie, then
the organization), and in addition checks the `Origin` header against the site's own origin and the configured
trusted origins, because a cookie is sent on a cross-site upgrade. A browser WebSocket cannot set the organization
header the web app uses, so the organization comes from the path or subdomain, or from `?org=` when the site is
configured for header tenancy.

Frames are JSON text.

| From the browser | Meaning |
|---|---|
| `{ "op": "sub", "topic": "chat:channel.7", "since": 41 }` | Join; `since` is the last sequence the browser has |
| `{ "op": "unsub", "topic": … }` | Leave |
| `{ "op": "event", "topic": …, "kind": "typing", "payload": {} }` | A client event; the kind must be in `client_events` |
| `{ "op": "ping" }` | Keep-alive; answered with `pong` |

| From the server | Meaning |
|---|---|
| `{ "t": "sub", "topic", "ok": true, "seq": 45, "presence": [...] }` | Joined (or `ok: false, "reason"`) |
| `{ "t": "msg", "topic", "seq", "event", "payload" }` | A published message |
| `{ "t": "event", "topic", "kind", "user", "payload" }` | A client event from someone else (never echoed to the sender) |
| `{ "t": "presence", "topic", "join" \| "leave": user }` | Someone connected or left |
| `{ "t": "resync", "topic" }` | Messages were missed beyond the kept history: read from the database |
| `{ "t": "bye", "reason" }` | The server is closing (shutdown, session ended); the browser reconnects |

**Resuming.** On `sub` with `since`, the server replays kept messages after that sequence, or answers `resync` if
they are gone. **Typing and other client events** are not kept and not replayed. They are rate-limited per
connection.

**Presence** counts connections per person: a person with two tabs is online once and leaves when the last closes.
A connection that stops answering pings is closed after `realtime.idle_secs`.

## Limits and safety

- Each browser tab opens one socket; a person may have `realtime.max_connections` (default 8).
- At most `realtime.max_topics` (default 100) topics per socket, frames up to 16 KB, client events at most
  `realtime.client_rate` per second (default 10) per socket; beyond that the frame is dropped and counted.
- A failed or slow receiver never blocks a publisher: each socket has a bounded queue, and a socket that cannot keep
  up gets `resync` for the topics it missed, then is closed if it still lags.
- The session is re-checked at the same interval as the event stream (`stream_lifetime_secs`); a signed-out person
  gets `bye`.
- Every socket is closed at shutdown, like the event stream.
- Joining, refusing and publishing are counted in the request log; message payloads are not logged.

## Several servers

Like the notification hub, the topic hub has one entry for messages (`publish`). A pub/sub backend would forward
local messages out and call `publish` for those from other servers; sequence numbers then come from the backend.
Until then one server owns all topics.

## In the web app

`web/src/lib/realtime/` holds one store per page load: it opens the socket on the first `subscribe`, counts
subscribers per topic, rejoins with `since` after a reconnect (exponential backoff, like the event stream), and
exposes `subscribeTopic(topic, handler)`, `sendEvent(topic, kind, payload)` and `presenceOf(topic)`. Widgets use it
(see [Custom pages](custom_pages.md)); plugins reach it through widget attributes, not code.

## Decide

| # | Question | Proposal |
|---|---|---|
| 1 | Is the message of record kept by the kernel or by the plugin? | **The plugin.** The socket carries signals; history, search and audit stay in the plugin's models. |
| 2 | Authorize by a plugin function or by a model rule? | **A plugin function**, called as the person; a rule on a model cannot express "is a member of channel 7". |
| 3 | WebSocket or keep extending server-sent events? | **WebSocket**, since typing and presence need the browser to talk back. The event stream stays for notifications. |
| 4 | Where do per-channel unread counts live? | **In the plugin** (a read cursor per person and channel); the kernel only signals. |
| 5 | How is the organization found on the upgrade? | Path or subdomain as today; **`?org=`** for header tenancy. |
| 6 | Does the first version need a pub/sub backend? | **No**; one server, with the seam in place. |
