---
title: "Aether Plugin Documentation"
description: "A plugin or addon is a component meant to extend the features of aether."
section: plugin
status: stable
order: 10
---
# Aether Plugin Documentation

A plugin or addon is a component meant to extend the features of aether.

There are 2 ways to create a plugin:
- Workspace
- Plugin

# Workspace

A workspace packages related plugins for load/install. On the Apps launcher it is
normally a **folder**: each member that declares
`[app]` keeps its own icon inside that folder. Set `launcher = "flat"` (Base) to
list each member as its own home-grid tile instead. Plugins may also set
`[app] folder = "Time"` to create or join a named folder within the same
workspace category. Modules with only one sidebar menu fold into the host app
unless `[app] standalone = true`.

Checked via `workspace.toml`:

```toml
[workspace]
name = "example"
label = "Example Workspace"
version = "0.0.1-beta"
description = "This is a description of an example"
long_description = """
This is the long description of an example
"""
# Folder on the Apps launcher. `group_icon` may be a Lucide id
# (`building-2`) or a path under the workspace (`./assets/example.svg`).
group_label = "Example"
group_icon = "./assets/example.svg"
# folder (default) | flat — flat lists each member on Apps home
# launcher = "folder"

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

Run `aether --gen` with no arguments for a wizard that asks whether you want a
plugin, a workspace or a config file, then the language, path, label,
description and author.

The last segment of `--plugin-path` is the plugin name (lowercase letters,
digits and `_`). Languages: `rhai` and `lua` (scripts, nothing to compile), `go`, `rust`, `typescript`,
`javascript`, `python`. No Extism CLI is needed. The command writes `plugin.toml`, a sample source file
(`main.rhai`, `main.lua`, or `src/main.go`, `src/lib.rs`, `src/main.ts`, `src/main.js`, `src/main.py`),
a `Makefile` (`make` builds `out/plugin.wasm`; scripts have none, as there is nothing to build), `README.md`, `.gitignore` and an
Business Source License 1.1 `LICENSE` (only outside a workspace, whose own license covers its plugins; non-production use only, production use needs a commercial license from you). Generated plugins declare no dependencies yet; the per-language SDKs
(`sdks/<language>`) are added by path. The Rust SDK is in `sdks/rust`, with examples in
`plugins/test`.

Pages are XML files under `pages/`; see [Visitors, public pages and the audit
trail](../architecture/access.md) and [Pages and data](pages.md) for showing a plugin's data.
Reusable UI fragments live under `components/`; see [UI components](components.md).
Theme plugins may also ship `layouts/<name>.json` and `errors/<name>.json`; see
[Themes, layouts and navigation](../architecture/themes.md).

What a plugin may ask the kernel to do (database, cache, files, web APIs, other plugins) is
described in [Kernel commands](commands.md); small script plugins in [Rhai plugins](rhai.md) and [Lua plugins](lua.md).
Three `plugin.toml` keys control reach beyond the plugin's own data: `http_hosts` (outside hosts
`http::request` may call), `dependencies` (plugins it may call with `plugins::call` and whose events
it may listen to) and `bridges` (integrations it may call with `bridge::call`, see [Bridges](../bridges.md)).
`[[events]]` declares what a plugin announces and what it listens to (see
[Events between plugins](../architecture/events.md)).
`[[watch]]` entries tell a plugin when a file arrives in its folder (see
[File watches](../architecture/watch.md)). `[[command]]` entries are commands an administrator runs with `aether --command`
(see [Plugin commands](cli-commands.md)). `[[schedule]]` entries declare recurring background tasks; see
[the scheduler](../architecture/scheduler.md). Sending email and SMS is one command,
[`communication::send`](../architecture/communication.md). `[[ai_tool]]` entries offer functions to the AI assistant
(see [AI](../architecture/ai.md)); asking a model, and OCR, are the commands `ai::complete` and `ocr::parse`.

A plugin becomes a tile on the Apps launcher by declaring `[app]`; see
[Organizations, switching, and the Apps launcher](../architecture/organizations.md).
Optional `[app] roles = ["hr_user"]` (and page `roles="…"`) hide the tile and
menus from members who do not hold one of those roles.
A page can name shell chrome with `layout="bare"` and a content wrapper with
`wrap="component_name"` on `<page>` (see [Pages](pages.md) and [Components](components.md)).
A plugin can also be a theme that decides the app's colours, layout and navigation; see
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
├── logs/                       process log when `[logging] file = true` (see [logging.md](../architecture/logging.md))
├── uploads/plugins/<stamp>/    plugin/workspace zips uploaded from Desk Apps
└── conf/                       other per-plugin configuration
```

`aether --load-plugins <package-or-workspace>…` registers packages in the core catalog.
Each argument may be a package directory, a workspace directory (`workspace.toml`), or a
workspace name such as `base` (resolved as `./base` or `./plugins/base` relative to the
current directory and the config file). A workspace expands to every member under
`[workspace.plugins]`, loaded in `dependencies` order from each `plugin.toml`. With no
arguments (`aether -c aether.toml --load-plugins`), it uses top-level `plugin_paths` in the
config file (paths relative to that file; workspace names work the same way).
`aether --load-plugins --dry-run` runs the same checks for every package and prints every
error, without writing to the catalog or `app_dir`. Foreign links may resolve against other
packages in the same run (their on-disk model ids), so a dependency does not need to be
catalogued already for the check to pass.

While the server is running, Desk Apps (`/desk/apps`) can do the same catalog load without
the CLI: **Reload from disk** re-reads `plugin_paths` from `aether.toml`, and **Upload zip**
stages a package or workspace archive under `uploads/plugins/` then loads it. Neither
upgrades organizations; use **Install into org** (or `--install-plugin` / `--upgrade-plugin`)
for that.

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
version it has until `aether --upgrade-plugin <name[@version]> --org <org_db>` (alias
`--upgrade`) moves it to the newest loaded one, or the one named, which is also how to go
back. `aether --upgrade all --org <org_db>` upgrades every installed plugin (dependencies
first). An upgrade needs the new version's dependencies installed and refuses page routes
another installed plugin serves.

## Documenting a plugin

A plugin can ship its own documentation. Put markdown files in a folder and add a `[docs]` table to
`plugin.toml`:

```toml
[docs]
dir = "docs"            # the folder with the .md files (default "docs")
visibility = "public"   # public | authenticated | org (default "authenticated")
```

Without the table the plugin has no docs, even if a `docs` folder exists. When the plugin is loaded
(`aether --load-plugins`) Aether reads the folder, checks it and stores the rendered pages with the plugin's
catalog entry, so the documentation travels with each version. They then appear on the
[documentation website](../architecture/running.md) under **Plugins**, in its search, in `/llms.txt`, and to LLM
tools through [the MCP server](../mcp.md).

**Pages.** `README.md` is the landing page; any other file is a page at its path (`guide/setup.md` is
`guide/setup`; `guide/README.md` is `guide`). A page may begin with a `---` block with `title`,
`description` and `order`; without a title the first heading is used. Link to another page with a relative
`.md` link (`[setup](guide/setup.md#install)`). Images must be `https://` addresses, because a plugin cannot
ship image files here. Raw HTML in the markdown is shown as text, never run. Limits: 100 files, 256 KB a
file, 1 MB in all.

**Who can read them.**

| `visibility` | Readers |
|---|---|
| `public` | Anyone who can reach the site. |
| `authenticated` | Any signed-in user. This is the default: publishing is a choice. |
| `org` | Members of an organization that has the plugin installed (developers see all). |

A page the reader may not see is a plain "not found", the same as a page that does not exist, and is left out
of the menu, search and `llms.txt`. When a newer version of the plugin has no `[docs]` table, the plugin is
no longer documented, whatever older versions had.

**Mistakes stop the load.** A link to a page that does not exist, an unsupported link scheme, a relative
image, a file that is too large, or two files with the same address make `aether --load-plugins` fail with
every problem listed, and nothing is stored. The MCP tool `validate_plugin` and `--dry-run` report them too.

**Preview while writing.** `aether --serve-docs --plugin-docs ./my_plugin` serves the docs of that folder at
once and reloads them every few seconds. Because that server has no sign-in it shows only `public` docs and
says in its log which plugins it is hiding; add `--docs-all` (only on `127.0.0.1`) to see them all.

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

