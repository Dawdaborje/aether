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
MIT `LICENSE`. Generated plugins declare no dependencies yet: the per-language
SDKs will be added to the templates once they exist.

Pages are XML files under `pages/`; see [Visitors, public pages and the audit
trail](../architecture/access.md).

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
├── plugins/<name>/<version>/   plugin.toml, the WASM artifact, page XML
├── views/<name>/<version>/     compiled page views (JSON)
└── conf/                       other per-plugin configuration
```

`aether --load-plugin <package-dir>` reads the package's `plugin.toml`, copies
the files it declares (`wasm_file`, page, model and theme files; nothing else,
so build output such as `target/` is left behind) into
`plugins/<name>/<version>/`, and registers the version in the core catalog.
Compiled views are written under `views/` the first time a plugin is compiled.
Relative paths in `aether.toml` are resolved against the config file.

`aether --install-plugin <name[@version]> --org <org_db>` then enables a
catalogued plugin for one organization.

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

