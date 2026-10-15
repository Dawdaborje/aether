# Mantle

Mantle is Aether’s adaptive working-set layer around the kernel: byte-weighted
retention, ghost keys, labeled entries, durable per-org heat, and a kernel-owned
warm set for data that barely changes but sits on every request.

Plugins still talk to **`cache::*`**. Docs, metrics, and the desk inspector talk
about **Mantle**. ZFS ARC is prior art only — never the product name.

| Surface | Name |
|---|---|
| Plugin / kernel commands | `cache::*` |
| Adaptive engine / metrics / desk | **Mantle** |
| Durable usage ledger | **Mantle heat** (`heat.json`) |
| Inspiration (not branding) | ZFS ARC |

## Key shape (`:` not `_`)

Kernel namespace (plugins cannot choose it):

```text
plugin:{org}:{plugin}
```

Fully qualified store key:

```text
{namespace}::{user_key}
```

Structured user keys (convention + helpers):

```text
plugin:{org}:{plugin}::row:{model}:{id}
plugin:{org}:{plugin}::list:{model}:{query_hash}
plugin:{org}:{plugin}::agg:{model}:{name}:{scope_hash}
plugin:{org}:{plugin}::meta:{name}
```

Use **`:`** as the segment separator. Underscores appear inside ids
(`user_profile`); joining with `_` is ambiguous to parse. Segments must not
contain bare `:` or control characters.

Default kind dignity for eviction pressure: `meta` > `row` > `agg` > `list`.

Helpers such as `cache::set_row` / `cache::get_row` build `row:{model}:{id}`.
Raw `cache::set(key, …)` remains an escape hatch.

## Live map, ghosts, heat

| Layer | Holds | Survives restart? |
|---|---|---|
| Mantle live map / Redis | Cached values | RAM: no · Redis: yes |
| Ghost map | Evicted key metadata only | No (RAM) |
| **heat.json** | Decayed scores + top keys | **Yes, on disk** |

Heat is not the value cache. It answers “what to prefer,” not “here is the blob.”
Do not dump every payload into `app_dir`.

### Heat path

```text
<app_dir>/orgs/<organization>/cache/mantle/heat.json
```

Not under `orgs/<org>/plugins/<plugin>/` — plugins must not rewrite their own
popularity. Atomic write: `heat.json.tmp` → rename over `heat.json`.

```json
{
  "v": 1,
  "org": "acme",
  "updated_at": "2026-10-12T22:00:00Z",
  "window_secs": 86400,
  "plugins": [
    { "plugin": "crm", "score": 1284.5, "hits": 9021 }
  ],
  "models": [
    { "plugin": "crm", "model": "contact", "kind": "row", "score": 880.2, "hits": 6400 }
  ],
  "top_keys": [
    { "plugin": "crm", "key": "row:contact:abc123", "score": 42.0 }
  ]
}
```

Scores use exponential decay. Flushes keep top-N only (plugins, models, keys).
On boot or org activate, heat seeds priority bias. Top `row:` / `meta:` keys are
enqueued for background refill **only** when a plugin registers a warmer via
`Cache::register_row_warmer` — Mantle never invents silent DB reads.

## Startup warm-set

Stable, hot data is kernel-owned (`kernel:…`). Plugins cannot `cache::clear` it.
Pin Tier A so plugin `list:` thrash cannot drop authz fixtures. Invalidate on
write paths — not TTL-only for Tier A/B.

Capability **string checks** (`require_cap`) are already an in-memory `HashSet`.
The expensive part is usually install membership and related org fixtures.

### Tier A — every authenticated / page / plugin path

| Entry | Why hot | Scope | Invalidate |
|---|---|---|---|
| `kernel::session:{token_hash}` | `find_session_by_token` every authed request | Global; key = hash of cookie | logout / revoke (sync); org switch; forced logout |
| `kernel::org_databases` | `identify()` tenancy check | Global | org create / delete |
| `kernel::user:{id}:memberships` | list orgs + `is_member` | Per-user | invite / remove / restore |
| `kernel:{org}::installed_plugins` | pages, i18n, apps, invoke | Per-org | install / enable / disable / upgrade (CLI writes `mantle_signals`; `--serve` polls) |
| `kernel::cap_catalog` | validate manifest keys | Global | process restart |
| `kernel:{org}::user:{id}:roles` | guards, `/me`, manage-users | Per-user+org | role grant/revoke; membership |

### Tier B — UI shell / page render

| Entry | Why hot | Scope | Invalidate |
|---|---|---|---|
| `kernel::plugin:{name}@{ver}:pages` | UI resolve | Global per version | catalog load / page publish |
| `kernel::plugin:{name}@{ver}:components` | wrap / `<use>` | Global per version | catalog load / component publish |
| `kernel::plugin:{name}@{ver}:i18n` | localized pages + menus | Global per version | i18n publish; install set |
| `kernel:{org}::theme` | tokens, layout, nav | Per-org | activate theme / theme edit |
| `kernel::plugin:{name}@{ver}:public` | anon invoke | Global per version | catalog publish |
| `kernel:{org}::apps_launcher` | desk/home apps | Per-org | install set / app menu publish |

### Tier C — bridges, settings, org admin

| Entry | Why hot | Scope | Invalidate |
|---|---|---|---|
| `kernel::settings:catalog` | settings desk + secret flags | Global | seed / migration / catalog admin |
| `kernel::setting:g:{key}` / `kernel:{org}::setting:{key}` | bridge / messaging lookups | Global + org | settings put |
| `kernel:{org}::profile` | owner + manage capability | Per-org | ownership / admin roles |

### Session-by-token (Tier A)

Cache `ValidSession` under `kernel::session:{token_hash}`. Soft TTL is a safety
net only. Correctness comes from synchronous bust on revoke.

| Hook | Action |
|---|---|
| `revoke_session_by_token` (logout) | delete cache entry before response |
| `set_session_org` | update `org_database_id` or invalidate |
| `create_session` | optional pre-warm |
| User deactivated / forced logout | invalidate that user’s sessions |

Use an in-process kernel hook on `AppState`, not the plugin event bus. With a
Redis Mantle backend, session entries (or a pub/sub revoke) keep multi-node
nodes coherent.

## Write generations and query caching

`db::get` rows, `db::find` lists, `db::count` and `db::aggregate` results are cached
per plugin namespace (`row:` / `list:` / `agg:`). Each entry is stamped with the
**write generation** of its table, held in `kernel:{org}::stamp:{table}` (Tier A
pinned). Every write through the host bumps the counter, so stale entries are
unreachable immediately — no prefix scan, and it also covers another plugin
writing the same table. `db::relate`, `db::unrelate` and `db::transitions` bump an
org-wide counter (`stamp:*`). A missing counter is re-created from the clock, so
an evicted counter can never repeat a value an old entry was stamped with.

A list/aggregate key is the SHA-256 of the exact SQL, its binds (row-scope values
included) and the fields hidden from the caller, so two callers share an entry
only when they would run the same statement and see the same fields. Writes made
outside this process (CLI, another node) are only covered by the short TTLs
(rows 10 min, queries 2 min).

### Reads stay audited

A cached read still writes its `data_access` row: the entry keeps the ids and row
count of the original read, and a hit inserts the audit row without the `SELECT`.
If that insert fails the read fails, as an uncached read would.

### One fill per missing key

Concurrent misses on the same row/list/aggregate queue behind one caller
(`Cache::flight`). The first runs the query and stores it; the others wake,
re-check the cache and use the stored entry. Flight entries clean themselves up.

### Other nodes

Each node has its own L1. A host write also inserts a `mantle_signals` row
(`kind = table_write`, `plugin = <table>`, `org`, `origin = <node id>`); every
other node's live query bumps that table's generation. Bursts on one table are
coalesced to the first write plus one trailing announcement per 250 ms, and a
node ignores its own signals. Disable with `coherence_signals = false` in
`[cache]` for a single-node deployment.

## Request path

Per request the kernel reads, from Mantle: the session (`kernel::session:*`, 5 min
soft TTL), org databases, the user's memberships (membership check is a lookup in
that list), installed plugins, roles, theme, visitor identity (`visitor:{hash}`,
5 min) and writes a visitor's "last seen" at most once a minute.

## Backends

| `[cache]` | Role |
|---|---|
| `backend = "moka"` (default) | In-process L1 (Mantle). |
| `[cache.redis]` alongside `moka` | L2: size-evicted entries are demoted, L2 hits promoted. |
| `[cache.memcached]` alongside `moka`, no Redis | L2, same behaviour. |
| `backend = "redis"` / `"memcached"` | Shared store only, no L1. |

```toml
[cache]
backend = "moka"
[cache.memcached]
servers = ["10.0.0.5:11211", "10.0.0.6:11211"]   # or host = / port =
key_prefix = "aether:cache:"
```

Memcached cannot list or delete by prefix, so `invalidate_prefix` / `clear` move the
whole key prefix to a new *epoch* (part of every stored key): everything written earlier
becomes unreachable and ages out. That drops more than the prefix asked for, never
less. Keys that are long or not printable ASCII are stored under their SHA-256. The
epoch is re-read at most once a second, which bounds how long another node's prefix
delete can go unnoticed. L2 deletes after a `set` go through the worker thread, not
the request.

## Reclaim

Moka weighs entries in plain bytes (key + value + 64; Tier A pins weigh 1), so its
capacity, the elastic target and the byte counters share one unit. Which entries go
first under pressure is decided by `maintain()` every 5 s: lowest dignity first
(`list` < `agg` < `row`; `meta`/kernel never), then scan-polluting lists, then the
coldest plugin/model by heat, then the adaptive recent/frequent lists. The adaptive
lists and the ghost directory are O(1) per operation (lazy-deleted queues and
incremental counters).

## Cross-process invalidation (CLI → server)

Mantle lives in the serving process’s memory. A separate `aether --load-plugins` /
`--upgrade-plugin` process updates SurrealDB but cannot touch that memory directly.

Instead the CLI (and catalog helpers) **emit** a row into the core table
`mantle_signals` (`kind` = `plugin_surface` or `installed`, plus plugin/version/org).
Every `--serve` (and any process with `AppState`) opens a Surreal WebSocket
`LIVE SELECT` on that table, busts the matching warm keys on create, and deletes
the row as an ack. A one-shot drain on connect catches anything written while the
listener was down; the live query reconnects if the stream ends.

Same-process paths (`--serve --watch`, Desk catalog reload) still invalidate Mantle
immediately; they also emit signals so other nodes stay coherent.

## Desk inspector

Developers open **`/desk/mantle`** (desk nav, superuser only).

`GET /api/desk/mantle/stats` returns system totals and per-org heat + live bytes:

- **System strip** — bytes used / max, entries, hit rate, kernel vs plugin bytes, backend
- **Kind mix** — `meta` / `row` / `list` / `agg` / `kernel`
- **Tier board** — A / B / C warm presence
- **Per-org table** — bytes cached, entries, heat age
- **Org detail** — hot plugins/apps and hot models with score, hits, and cached bytes

No raw session tokens in the UI; session line is a count (+ bytes) only.

## Steal catalog (ZFS ARC → Mantle)

### Phase 1 (this design)

Colon labels, byte budget, ghost directory, heat.json, kind dignity, stats.

### Phase 2

| Piece | Status |
|---|---|
| Elastic byte target + async reclaim | **In** — shrinks under ghost-hit pressure; gradual slice (~5% of target / ≤32 keys per tick) |
| Scan-pollution marks | **In** — repeat `list:` ghosts inflate weigher weight |
| L1 Mantle → L2 Redis demote | **In** — `backend = moka` + `[cache.redis]` demotes size-evictions; L2 hits promote |
| Recency/frequency dual lists + adaptive target | **In** — shadow T1/T2 + ghost `p` bias reclaim only (Moka TinyLFU stays) |
| In-use soft pins | **In** — recent touches skipped by async reclaim (~2s TTL) |

### Phase 3

| Piece | Status |
|---|---|
| Compress large values | **In** — deflate pack (`MZ1\0`) when ≥ `compress_min_bytes` (default 4KiB) and smaller |
| Active row warmup hooks | **In** — heat `top_keys` → queue; refill via registered `WarmupFetcher` only |
| Active `db::get` row fill | **In** — ACL-safe cache-aside when no row-scope SQL / hidden fields / expand; bust on update/delete |

### Do not steal

Block checksums in cache, ZFS buffer plumbing, branding anything “ARC.”

## Defaults

- Engine **Mantle**, API **cache**, heat file **JSON**
- Path `<app_dir>/orgs/<org>/cache/mantle/heat.json`
- Separator `:`
- Kind dignity `meta` > `row` > `agg` > `list`
- Lazy org heat load; best-effort flush on shutdown

## Out of scope (first cut)

- Persisting full cache payloads into `app_dir`
- Plugin-writable heat files
- Blocking boot until every org’s heat is loaded
- Replacing Moka TinyLFU with a full ARC dual-list admission policy

See also [Caching layer](cache.md) for the plugin-facing `cache::*` surface.
