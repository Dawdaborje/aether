# Logging

Aether logs through the `log` crate; choose the level with `--log` (`error`,
`warn`, `info`, `debug`, `trace`; the default is `debug`).

## Every request

Each HTTP request is logged by `aether_core::request_log`, which also gives it
an id. The same id is the `request_id` of its audit rows (see
[access.md](access.md)) and is returned in an `x-request-id` response header, so
a log line, an audit row and a browser's network tab can be matched. A
client-supplied `x-request-id` is ignored.

| Level | What |
|---|---|
| `debug` | the request as received: method, URI, client address, headers; and the response headers |
| `info` | one line per response: `<id> GET /path -> 200 OK in 4ms (ip, size, user agent)`; also `401` (the normal "log in" answer) |
| `warn` | other `4xx` responses |
| `error` | `5xx` responses |
| `trace` | successful static files under `/web` |

Secrets are never written: the `cookie`, `set-cookie`, `authorization` and
similar headers, and query values such as `code`, `state` or `token`, appear as
`<redacted>`. Request and response bodies are not logged.

## What happened, not just the status

The handlers log their decisions with the same id, so a `404` or `401` explains
itself:

- **Identity:** whether there was a session, which organization was resolved,
  and, when none could be, why (tenancy mode, candidates tried, whether the
  caller was logged in) and how to fix it. `identify finished in Nms` shows the
  time spent resolving the caller.
- **Pages:** which plugin and route matched (public or private, with URL
  parameters), or that no installed plugin serves the page, with the plugins
  that are installed.
- **Plugin calls:** `plugin.function in organization by actor: ok | refused`.
- **Login:** success or failure for a username (never the password).
- **Rate limits:** requests refused with `429` (they are logged, but not written
  to the audit trail).
