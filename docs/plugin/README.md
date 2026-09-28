# Aether Plugin Documentation

A plugin or addon is a component meant to extend the features of aether.

There are 2 ways to create a plugin:
- Workspace
- Plugin

# Workspace

A workspace is a of grouping a bunch of related plugins.

it is always been checked with the `workspace.toml` file

```toml
[workspace]
name = "example"
label = "Example Workspace"
version = "0.0.1-beta"
description = "This is a description of an example"
long_description = """
This is the long description of an example
"""

```


# Plugin

## WASM Runtime

Register each immutable plugin version in the core `plugins` catalog with an
`artifact_path` relative to `app_dir`, an optional SHA-256 `artifact_hash`, and
matching `name` and `version` metadata. Store the package's `plugin.toml` beside
the WASM artifact. At startup the core compiles every active catalog version;
the artifact path and manifest must resolve inside `app_dir`.

An organization invokes only the version pinned in its `installed_plugins`
record. The server creates a fresh Extism instance from the cached compiled
module for each call, keeping mutable WASM state isolated between organizations.
Calls use `POST /api/plugins/{plugin}/{function}` with a JSON body; the session
cookie selects the organization. Plugin exports receive the JSON body and return
JSON when possible (otherwise the response is returned as a string).

Plugins can call the host import `aether::command` with one JSON string argument
and a JSON string result. The request shape is `{ "command": "events::emit",
"payload": { "event": "notice", "payload": { "message": "Saved" } } }`.
Host commands are capability-checked from the plugin manifest. `events::emit`
publishes to the core notification websocket, scoped to the organization running
the plugin. The websocket endpoint is `/api/ui/ws/notifications` and requires
the authenticated session cookie.

