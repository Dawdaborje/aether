---
title: "Plugin loading"
description: "Plugins are WebAssembly modules run by Extism. Compiling one is the expensive step, so Aether never compiles at start-up: a plugin is compiled the first time it is called, kept ready while there is…"
section: architecture
status: stable
order: 330
---
# Plugin loading

Plugins are WebAssembly modules run by Extism. Compiling one is the expensive step, so
Aether never compiles at start-up: a plugin is compiled the first time it is called, kept
ready while there is room, and cached on disk so it does not have to be compiled again.

## Two tiers

```
call → memory (moka, bounded in MB)
         miss → one load per plugin → disk (wasmtime compile cache, bounded in MB)
                  miss → compile → written to disk → kept in memory
```

- **Memory.** Compiled plugins stay ready until the memory budget is full; then the least
  recently used goes. Every plugin version is its own entry, so two organizations on
  different versions of a plugin do not collide. Running instances are never cached: a
  fresh one is made for each call.
- **Disk.** wasmtime's compilation cache, on by default under `app_dir/cache/compiled`.
  A restart, or a plugin evicted from memory, then loads without compiling again, which
  removes the compile spike. Aether writes the wasmtime config file itself
  (`app_dir/conf/wasmtime-cache.toml`) so the location and size limit are the ones you set.
  Without this, Extism would use wasmtime's system-wide default location.

Turn the disk tier off with `enabled = false`; memory is then the only tier.

## Configuration

Every size is in megabytes.

```toml
[plugin_runtime]
max_compiled_memory_mb  = 256   # budget for compiled plugins kept ready
max_wasm_size_mb        = 24    # largest .wasm accepted
compiled_size_factor    = 8.0   # estimated compiled size = wasm size x this ...
engine_overhead_mb      = 2.0   # ... + this, per plugin
max_concurrent_compiles = 1     # compilations at once; each can need several times the plugin's size
compile_queue_limit     = 32    # compilations allowed to wait; beyond this callers get 503
compile_timeout_secs    = 60
instance_memory_mb      = 16    # memory limit of one running call
max_concurrent_calls    = 64    # calls running at once
call_fuel               = 50000000  # wasm instructions one call may use (kernel commands not counted)
call_timeout_secs       = 10    # wall time of one call, kernel commands included
outbound_calls_per_minute = 600 # http::request + bridge::call a plugin may start per minute, per organization (0 = no limit)
outbound_max_concurrent   = 16  # outgoing calls one plugin may have waiting at once, per organization (0 = no limit)

[plugin_runtime.compile_cache]
enabled     = true
directory   = "cache/compiled"  # relative to app_dir
max_size_mb = 1024              # disk budget
```

Everything is optional; the values shown are the defaults. Unknown keys and values that
cannot work together (for example a `max_wasm_size_mb` whose compiled estimate exceeds
`max_compiled_memory_mb`) stop the server from starting, with a message saying why.

## How big is a compiled plugin?

wasmtime cannot report the size of a compiled module, so it is estimated:
`wasm size x compiled_size_factor + engine_overhead_mb`. The defaults come from measuring
synthetic modules: each extra plugin kept ready cost 9 to 12 times its `.wasm`, plus about
2 MB for its own engine. Real code is usually less dense, so 8 leans safe. Re-measure with
a real plugin:

```text
cargo test -p aether_core --lib calibrate -- --ignored --nocapture
```

(edit `calibrate_compiled_size` in `plugin_manager/runtime.rs` to load your `.wasm`).

A plugin whose estimate alone exceeds the budget is **refused** with an error naming it, its
estimate and the budget; it is not run uncached. One over `max_wasm_size_mb` is refused
before it is read.

## Load flow

1. Memory hit: use it.
2. Miss: callers asking for the same plugin and version at the same time share one load.
3. Read the catalog record; check the size limits and the artifact's hash.
4. Wait for a compile turn. More than `compile_queue_limit` waiting, or a wait longer than
   `compile_timeout_secs`, answers 503 "plugin is busy, retry shortly".
5. Compile (or read from disk), then keep it in memory, evicting least recently used.

A call also waits for one of `max_concurrent_calls` slots; if none frees up it gets 503.

When a load fails at any step (a missing or tampered file, a size limit, a compile error, a full
queue or a timeout), the reason is kept for that plugin version until a load of it works. The
plugin's page in the desk (**Apps**, then the plugin) shows it under **Problems**, with how many
times in a row it failed, and lists the plugin's recent installs, upgrades and loads that failed
in any organization (from the [plugin activity](plugin_loading.md#watching-it) log). The reason
is held in memory, so it is gone after a restart; the activity log is permanent, and a version that has not been loaded since the restart still shows its last recorded install or upgrade failure (until a later one works). A red **Load failed** badge marks the plugin on the Apps catalog and in an organization's Plugins
list (hover for the reason). The detail call
is `GET /api/ui/catalog/{name}/{version}/details`, with `load_failure` and `recent_failures`; the
catalog list and an organization's plugin list carry `load_failure` on each entry.

## Memory you should expect

`peak ~= max_compiled_memory_mb + max_concurrent_calls x instance_memory_mb
         + max_concurrent_compiles x (compile spike)`

In-flight calls keep their plugin alive after it is evicted, so the cache can briefly be
over budget by what is running.

## Watching it

`GET /api/ui/plugins/runtime` (developers only) shows the memory in use against the
budget, plugins ready, hits, loads, evictions, refusals, queue rejections, timeouts,
average load time and whether the disk tier is on. Many evictions with the same plugins
loading again and again mean the budget is too small.

## Versions and revisions

A plugin's `(name, version)` is the cache key, and versions are immutable, so there is
nothing to invalidate. Each organization runs the version pinned in its
`installed_plugins` record; several versions can be ready at once, each counted separately.

Plugin files are stored as revisions that share what did not change (see the plugin README):
each load writes only the files whose hash differs into a new time-stamped folder, and
a rebuild of a catalogued version becomes `<version>+<stamp>`. The runtime finds a version's
`plugin.toml` and artifact through its revision's `files.json`, so a version can be built from
files that live in an older revision's folder.
