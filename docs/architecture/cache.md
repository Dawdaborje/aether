---
title: "Caching layer (cache::)"
description: "Plugins reach a namespaced key/value store through kernel commands only: cache::get, cache::set, cache::invalidate, cache::clear (and structured helpers such as cache::set_row / cache::get_row)."
section: architecture
status: stable
order: 90
---
# Caching layer (`cache::*`)

Plugins reach a namespaced key/value store through kernel commands only:
`cache::get`, `cache::set`, `cache::invalidate`, `cache::clear` (and structured
helpers such as `cache::set_row` / `cache::get_row`). Values are JSON. The
kernel scopes every key to the active organization and plugin:

```text
plugin:{organization}:{plugin}::{key}
```

Another plugin or organization can neither read nor clear those entries. Keys
are at most 256 bytes; a value must fit `max_value_bytes`. The cache may drop
entries at any time (memory pressure, restart, another node when Redis is not
used), so it holds things that can be recomputed — never the only copy of data.

## Backends

Configured under `[cache]` in `aether.toml`:

| Backend | Role |
|---|---|
| **Moka** (default) | In-process concurrent cache |
| **Redis** | Shared across processes / scheduler workers |
| **Memcached** | Shared across processes; `[cache.memcached]`, prefix delete = epoch bump |

When plugins use `cache::*` from background jobs and from HTTP, prefer Redis so
a value written in one process is visible in another. The in-process backend is
private to each process.

## Policy and heat: Mantle

Eviction policy, byte budgets, ghost keys, per-org heat on disk, kernel warm-set
(sessions, installed plugins, settings, …), and the desk inspector are documented
in [Mantle](mantle.md). Prefer colon-structured keys (`row:`, `list:`, `meta:`)
so Mantle can meter and invalidate by model.
