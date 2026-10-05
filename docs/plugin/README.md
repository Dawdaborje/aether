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

[workspace.plugins]
company = { path = "./company" }
```

Member plugins are declared in `[workspace.plugins]`, as `name = { path = "..." }`
with the path relative to `workspace.toml`. This is the only supported form;
`[[addons]]` is rejected.

## Creating a plugin

```sh
aether --gen plugin --plugin-path company --plugin-language rust
```

The last segment of `--plugin-path` is the plugin name (lowercase letters,
digits and `_`). Languages: `go`, `rust`, `typescript`, `javascript`, `python`.
No Extism CLI is needed. The command writes `plugin.toml`, a sample source file
(`src/main.go`, `src/lib.rs`, `src/main.ts`, `src/main.js` or `src/main.py`),
a `Makefile` (`make` builds `out/plugin.wasm`), `README.md`, `.gitignore` and an
Business Source License 1.1 `LICENSE` (non-production use only; production use needs a commercial license from you). Generated plugins declare no dependencies yet; the per-language SDKs
(`sdks/<language>`) are added by path. The Rust SDK is in `sdks/rust`, with examples in
`plugins/test`.

Pages are XML files under `pages/`; see [Visitors, public pages and the audit
trail](../architecture/access.md) and [Pages and data](pages.md) for showing a plugin's data.

What a plugin may ask the kernel to do (database, cache, files, web APIs, other plugins) is
described in [Kernel commands](commands.md); small script plugins in [Rhai plugins](rhai.md).
Two `plugin.toml` keys control reach beyond the plugin's own data: `http_hosts` (outside hosts
`http::request` may call) and `dependencies` (plugins it may call with `plugins::call`).
`[[schedule]]` entries declare recurring background tasks; see
[the scheduler](../architecture/scheduler.md). Sending email and SMS is one command,
[`communication::send`](../architecture/communication.md).

A plugin becomes a tile on the Apps launcher by declaring `[app]`; see
[Organizations, switching, and the Apps launcher](../architecture/organizations.md).
A page can name its layout with `layout="bare"` on `<page>`, and a plugin can be
a theme that decides the app's colours, layout and navigation; see
[Themes, layouts and navigation](../architecture/themes.md).

When an ancestor directory holds a `workspace.toml`, the plugin is also added to
its `[workspace.plugins]` table, like `cargo new` does for a Cargo workspace, and
`[plugin.meta] workspace` is set to the workspace name.

# Plugin

## `app_dir`

`app_dir` (`app_dir = "..."` in `aether.toml`, or `--app-dir`) is where Aether
keeps everything a plugin needs on disk:

```text
<app_dir>/
├── plugins/<name>/<stamp>/     one revision: only the files that changed, and files.json
├── views/<name>/<version>/     compiled page views (JSON)
└── conf/                       other per-plugin configuration
```

`aether --load-plugin <package-dir>` reads the package's `plugin.toml`, checks that every
capability it asks for exists (`capabilities/`), and registers it in the core catalog.

Files are stored as **revisions**. Each file is compared by SHA-256 with the plugin's latest
revision; only the files that differ are written, into a new folder named after the date and
time (`plugins/<name>/20261003T210100Z/`). The folder's `files.json` lists every file of the
revision with its hash and the folder that holds its bytes, so nothing is ever copied twice.
Loading identical content again does nothing. Loading changed content under a version that is
already catalogued adds `<version>+<stamp>` (for example `0.1.0+20261003T210100Z`), so a rebuild
needs no manual version bump; the first load of a version keeps the version as written.
Compiled views are written under `views/` the first time a plugin is loaded.
Relative paths in `aether.toml` are resolved against the config file.

`aether --install-plugin <name[@version]> --org <org_db>` then enables a catalogued plugin for
one organization (without a version: the one loaded most recently). An organization keeps the
version it has until `aether --upgrade-plugin <name[@version]> --org <org_db>` moves it to the
newest loaded one, or the one named, which is also how to go back. An upgrade needs the
new version's dependencies installed and refuses page routes another installed plugin serves.

## WASM Runtime

Register each immutable plugin version in the core `plugins` catalog with an
`artifact_path` relative to `app_dir`, an optional SHA-256 `artifact_hash`, and
matching `name` and `version` metadata. Store the package's `plugin.toml` beside
the WASM artifact. Plugins compile on first use, not at start-up; the artifact path and
manifest must resolve inside `app_dir`.

How plugins are compiled, cached and limited is described in
[plugin loading](../architecture/plugin_loading.md).

An organization invokes only the version pinned in its `installed_plugins`
record. The server creates a fresh Extism instance from the cached compiled
module for each call, keeping mutable WASM state isolated between organizations.
Calls use `POST /api/plugins/{plugin}/{function}` with a JSON body; the session
cookie selects the organization. Plugin exports receive the JSON body and return
JSON when possible (otherwise the response is returned as a string).

When a plugin function fails, the caller normally gets a generic `502` and the detail goes
to the server log. An error whose message starts with `aether:user-error:` is for the
caller instead: it is returned as `422` with the text after the marker (the SDK's
`Error::msg` does this).

Plugins can call the host import `aether::command` with one JSON string argument
and a JSON string result. The request shape is `{ "command": "events::emit",
"payload": { "event": "notice", "payload": { "message": "Saved" } } }`.
Host commands are capability-checked from the plugin manifest.

- `events::emit` sends a transient event to the organization's connected members over
  the notification stream. It is not stored.
- `notify::send` stores a notification and pushes it to whoever is connected; see
  [notifications](../architecture/notifications.md). Payload: `{ "title", "body"?, "link"?,
  "level"?, "payload"?, "expires_in_secs"?, "audience"? }`. The audience is `"members"`
  (the default), `"caller"` (whoever made this request, a visitor included),
  `{ "actors": ["users:…", "visitors:…"] }`, or `"everyone"`, which also needs the
  `notify::public` capability. Declare `notify::send` (and `notify::public`) in the
  manifest's capabilities.

