# Notifications

Notifications are driven by the kernel and delivered with server-sent events (SSE):
one-way, over plain HTTP, one connection per browser tab. Nothing travels from the
browser to the server on that connection; marking as read and listing are ordinary
requests.

## How it works

1. The kernel (`state.notify(org, …)`) or a plugin (`notify::send`) creates a
   notification. It is stored in the organization's `notifications` table first.
2. The `NotificationHub` then wakes the browsers connected to that organization.
   The hub holds one in-memory channel per organization, created when its first browser
   connects and dropped when the last leaves.
3. Each stream filters by audience, so a browser only receives what is for it.

The table is the source of truth. Every notification has a time-ordered `ulid`, which is
the SSE `id:`. A browser that reconnects sends `Last-Event-ID` and the server replays what
it missed (up to `replay_limit`). A browser too slow for the channel gets a `resync` event
and refetches the list.

## Who receives what

| Audience | Who |
| --- | --- |
| `members` | Logged-in members of the organization |
| `everyone` | Members and anonymous visitors |
| `actors` | Specific `users:…` / `visitors:…` ids |

Anonymous visitors can listen. A browser with no visitor cookie only gets what is for
everyone and is not given an identity for listening. A visitor with a cookie also gets what
is addressed to them, and read marks are recorded per actor. A logged-in user who is not a
member of the organization is treated as a visitor.

Plugins need `notify::send`; sending to `everyone` also needs `notify::public`.
`audience: "caller"` answers whoever made the request, which is how a public page can
notify its own visitor.

## Endpoints

- `GET /api/ui/notifications/stream`: the event stream. Events: `notification` (with
  `id`), `event` (a transient plugin event, members only), `resync`, and `reconnect`
  (the server ends the stream after `stream_lifetime_secs`; the browser reopens it, which
  checks its session again).
- `GET /api/ui/notifications?unread=1&limit=50`: the list, newest first, with `unread`.
- `POST /api/ui/notifications/read` with `{ "ids": [...] }`, or `{}` for all.

The web app reads the stream with `fetch` rather than `EventSource`, because header
tenancy needs the `X-Org-Slug` header, which `EventSource` cannot send.

## Configuration

```toml
[notifications]
max_streams_per_actor = 8     # open streams per person (or per address, when anonymous)
keepalive_secs = 25           # keep-alive comment interval
stream_lifetime_secs = 900    # server closes the stream; the browser reconnects
replay_limit = 100            # most notifications replayed on reconnect
# retention_days = 90         # delete older notifications; expired ones always go
```

Every value is optional; the ones shown are the defaults. A cleanup task removes expired
(and, with `retention_days`, old) notifications hourly. Open streams are closed at
shutdown so they do not hold the server up.

## Several servers

The hub is in-process. With one server that is complete. With several, a notification
created on one server would not reach a browser connected to another, even though the
table is correct. `NotificationHub::publish` is the one entry point for messages, so a
pub/sub backend (for example one SurrealDB `LIVE SELECT` per organization per server, or
Redis) belongs there: forward local messages out and call `publish` for messages that
arrive.
