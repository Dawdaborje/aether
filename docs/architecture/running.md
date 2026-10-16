---
title: "Running Aether"
description: "aether --serve needs an initialized database (aether --init once for a fresh install; it reports clearly if you forgot)."
section: architecture
status: stable
order: 380
---
# Running Aether

## Starting and stopping

`aether --serve` needs an initialized database (`aether --init` once for a
fresh install; it reports clearly if you forgot). After upgrading Aether,
apply new core migrations with `aether --migrate` (migrations only — no
re-seed and no new developer account).

Ctrl+C or SIGTERM shuts it down gracefully: it stops accepting connections,
lets in-flight requests finish (for up to 10 seconds, since open event streams (SSE)
would otherwise hold it open), stops background tasks, signs the
database session out and exits with status 0. All requests share one websocket
to SurrealDB, which closes when the process exits.

Process logs go to stderr by default; set `[logging] file = true` (or
`--log-file` / `--log-dir`) to also append under `app_dir`. See
[logging.md](logging.md).

## The documentation website

These docs are compiled into the `aether` binary, so they are available wherever it runs, with no
source tree, no config file and no database.

```sh
aether --serve-docs                  # http://127.0.0.1:7899
aether --serve-docs 0.0.0.0:8080     # another address
```

`aether --serve` also serves the same site, at `/docs` by default (`http://localhost:7890/docs`).
Change or turn it off in `aether.toml`:

```toml
[docs]
enabled = true     # false: do not serve the documentation under --serve
path = "/docs"     # where it is mounted; "/" is refused because the API uses it
```

The site has a search box (`/search?q=...`), a light and a dark theme, and every page as plain
markdown by adding `.md` to its address (`/plugin/pages.md`). `/llms.txt` lists all pages and
`/llms-full.txt` is the whole documentation in one file, for language models. Pages marked
*proposal* describe designs that are not built yet; the site and both text files say so.

## Loading plugins while the server runs

On Desk Apps (`/desk/apps`), developers can **Reload from disk** (re-reads
`plugin_paths` in `aether.toml` and loads into the catalog) or **Upload zip**
(a package or workspace archive). That only updates the catalog — install or
upgrade into organizations stays a separate step. For auto-upgrade of every
org that already has a plugin when sources change, use `--serve --watch`
instead (development only).

## The developer account

The account `--init` creates is the **developer**: it bypasses permission
checks, lands on the desk (the Organizations page) after login, and is the only
account that sees the desk and the developer tools (`/desk/studio`, more to come); see
[organizations.md](organizations.md). It is stored as a superuser
(`is_super_user`); the API reports it as `is_developer` too.

## Developing plugins: `--watch`

`aether --serve --watch` reloads plugins when their source changes, so you edit and try without loading
and upgrading by hand. The catalog remembers the folder each plugin was loaded from (`aether
--load-plugins <folder>`); the server listens to those folders and, half a second after a change settles
(an editor or a build writes several files), does what you would do yourself: loads the folder again
(identical content is recognized and ignored; changed content becomes a new revision of the same version,
such as `0.1.0+20261005T161642Z`) and moves every organization that has the plugin installed to it. The next
call to the plugin runs the new code, and its models, pages, schedules, watches and event listeners follow.

* A change that does not load (a script that does not compile, a manifest error, a model change the data
  cannot take) is logged as an error, once, and the running version is left alone. Fix it and save again.
* Changes to hidden files and to `target/`, `node_modules/` and editor scratch files are ignored, and
  reading a plugin's files never counts as a change.
* A plugin loaded after the server started is picked up within ten seconds.
* This is for development: it moves **every** organization that has the plugin to the new revision, so
  do not use it on a server with real data.
