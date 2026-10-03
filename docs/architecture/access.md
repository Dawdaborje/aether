# Visitors, public pages and the audit trail

Plugins can serve pages and functions to people who are not logged in. Anonymous
access is opt-in per page and per function, every anonymous caller has an
identity, and everything they do is recorded.

## Declaring public pages

A page is an XML file under the plugin's `pages/` folder. Its root element
declares the route and who may open it:

```xml
<page route="/messages" public="true" title="Messages" model="message">
  <view type="list" model="message"> ... </view>
</page>
```

- `route` is required: absolute, with segments of letters, digits and `- _ .`.
  `/` is a valid route.
- A segment written `{name}` is a parameter that matches one non-empty path
  segment: `route="/chat/{channel}"` serves `/chat/general` with
  `params = { channel: "general" }` (returned by the page API and passed to the
  page as `page.params`). Names are lowercase letters, digits and `_`, and there
  are no catch-all parameters. Treat parameter values as untrusted input.
- When several pages match a URL, an exact route beats a pattern, and among
  patterns the one with more literal segments wins (`/chat/new` over
  `/chat/{channel}`).
- Two pages cannot match the same URLs. Routes with the same *shape* conflict,
  so `/chat/{channel}` and `/chat/{room}` do; `/chat/new` and `/chat/{channel}`
  do not. The check covers one plugin when it is loaded, and the plugins already
  installed when you install another into an organization.
- `public` is `"true"` or `"false"`. Anything else is an error, and the default
  is `false`: **private**.
- There is no page table in `plugin.toml`. A `[[pages]]` table is rejected.
- Every model a page names (`model="..."`) must be declared by the plugin, in
  `[[models]]` or an access list. Pages are validated when the plugin is loaded.

## What an anonymous visitor can do

By default: read the models that the plugin's public pages show, and nothing
else. Anything more is declared in `plugin.toml`:

```toml
[plugin]
capabilities        = ["db::query", "db::mutate"]
public_functions    = ["post_guestbook"]            # callable by anonymous visitors
public_capabilities = ["db::mutate"]                # subset of `capabilities`
public_access_models = [{ name = "guestbook", permissions = ["write"] }]
```

- Public pages grant read access (`db::query`) to the models they name.
- `public_functions` lists the functions anonymous callers may invoke; the check
  happens before any WASM is compiled.
- Writing needs both a `public_capabilities` entry (`db::mutate`) and a model
  entry with `write`. A model listed with `write` is writable by anyone.
- There is no raw SurQL for anyone: plugins reach the database only through the
  structured `db::*` commands, on the models they were granted.

## Visitors

The first request to a public page (or public function) creates a `visitors`
row in the organization database and sets an HttpOnly `aether_visitor` cookie
(`Secure` unless `is_development_mode`). The row's id is the actor in the audit
trail, so everything one browser did can be followed. When that browser logs in,
the visitor is linked to the user (`visitors.linked_user`).

A logged-in user from a different
organization is treated as a visitor on public pages and gets `403` on private
ones.

Anonymous requests have no session to name an organization, so the organization
comes from tenancy (`[tenancy] org_resolution` = `subdomain`, `path` or `header`)
and must exist in `org_databases`. With `session_only`, only logged-in users are
served.

## Rate limits

Two per-client-address limits apply to anyone who is not a logged-in user, over
a one-minute window held in memory (per server process):

- `[public] max_requests_per_ip_per_minute` (default 300): page and plugin-call
  requests. Over the limit the answer is `429` with `Retry-After: 60`. Requests
  without a session cookie are refused before any database work. Logged-in
  users are not limited.
- `[public] max_new_visitors_per_ip_per_minute` (default 30): new visitor
  identities, since each is a stored row.

Refused requests are not written to the audit trail, so a flood cannot also
become a flood of audit rows. The client address is the TCP peer unless the peer
is listed in `[server] trusted_proxies`; put a proxy's address there if one sits
in front of Aether, or every visitor shares one budget.

## The audit trail

Written to the organization database, append-only, by the kernel only:

| Table | Records |
|---|---|
| `page_visits` | actor, plugin, route (the page as declared, e.g. `/chat/{channel}`), path (what was requested, e.g. `/chat/general`), status (including 401/403/404/429 refusals), IP, user agent |
| `plugin_calls` | actor, plugin, function, status, IP |
| `data_access` | actor, plugin, function, model, table, operation (`read`/`create`/`update`/`delete`), **record ids**, count, IP |

Rows for one request share a `request_id`. Every `db::*` command writes its
`data_access` row in the same transaction as the work, so there is never an
access without a record. **If an audit row cannot be written, the request fails**
and the change is rolled back. `db::find` returns at most 1000 rows.

Plugins cannot map a model onto the audit tables (or other kernel tables). Field and
order names must be plain identifiers and values are always bound as values, so a
plugin cannot shape a query beyond what the structured commands allow.

### Configuration (`aether.toml`)

```toml
[server]
trusted_proxies = ["10.0.0.1"]   # only these may set X-Forwarded-For

[audit]
ip = "full"                      # "full" | "truncated" | "hashed"
ip_hash_key = "..."              # required for "hashed" (16+ characters)
retention_days = 365             # omit to keep forever

[public]
max_requests_per_ip_per_minute = 300
max_new_visitors_per_ip_per_minute = 30
```

- `truncated` keeps the IPv4 `/24` or IPv6 `/48`; `hashed` stores a keyed hash
  (rows from one address still match each other, the address cannot be read back).
- `retention_days` deletes older audit rows and long-unseen visitors, hourly while
  the server runs. `aether --purge-audit` does it once and exits.
- Without `trusted_proxies`, the TCP peer is the client address, so the header
  cannot be used to forge audit entries.
