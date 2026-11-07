---
title: "API tokens, record API and webhooks"
description: "Today the only credential is the browser session cookie (aether_session), and the only public entry to plugin code is POST /api/plugins/{plugin}/{function}."
section: architecture
status: stable
order: 40
---
# API tokens, record API and webhooks

**Status: built**, apart from the few things listed under "Not built": [API tokens](#api-tokens), the [record API](#record-api)
with its generated OpenAPI description, outgoing [webhooks](#webhooks) and [inbound hooks](#inbound-hooks).
**Not built:** plugin commands to manage webhooks (`webhooks::*`), the `Idempotency-Key` header, an IP allow-list per token,
"recent second factor" before making a token, verification by a provider-specific bridge (Stripe, GitHub) for inbound hooks,
and a hook that queues its work and answers `202` at once. Choices are marked *Decide*
in the last section.

Before this, the only credential was the browser session cookie (`aether_session`), and the only entry to plugin
code was `POST /api/plugins/{plugin}/{function}`. A script, another system or a mobile client had no way in. This
page adds **API tokens** and a **record API** on the same rails: every call is made as a person, through the same
rules, and audited.

## API tokens

A token is a credential for one person in one organization, limited on purpose. It can never do more than that
person can: the rules, roles and capabilities are worked out for the owner exactly as for a browser, and the token's
scopes only narrow it. Removing a role from the person takes effect on the next call; deactivating the person ends
every token of theirs (within 30 seconds on other servers, see below).

### Making and using one

A signed-in person (a token cannot make tokens) uses the endpoints, which are also what a profile page would call:

```
POST   /api/ui/tokens   { "name": "nightly sync", "scopes": { ... }, "expires_days": 30 }
                        -> 201 { "id", "token": "aet_...", "expires_days", "note" }   // the token is shown once
GET    /api/ui/tokens   -> { "tokens": [ { id, name, prefix, scopes, created_at, expires_at, last_used_at, last_ip, revoked_at } ] }
DELETE /api/ui/tokens/{id}
```

The token is sent as `Authorization: Bearer aet_...`. It is `aet_` and 64 hex characters, so a leaked one is
recognisable. Only its SHA-256 is stored (in the core database, `api_tokens`); a person has at most 50 live tokens
per organization. A token made without `expires_days` lasts 90 days; the longest allowed is `api.token_max_days`
(365; 0 allows tokens that never expire).

### Scopes

A token with nothing in its scopes does nothing; at least one entry is required.

```json
{ "plugins":   ["crm"],                       // may call any function of the plugin (/api/plugins/crm/...)
  "functions": ["hr.hire"],                   // or single functions, as plugin.function
  "models":    ["crm.lead:read,update", "crm.deal:write"] }   // record API: read, create, update, delete; write = create+update+delete
```

Calling functions and using the record API are separate grants: a `plugins` scope does not open the record API, and a
`models` scope does not open functions.

### Where tokens are accepted

**Only** on `/api/plugins/{plugin}/{function}` and `/api/v1/...`. Every other endpoint (the pages, chatter, record
titles, media, organization administration) does not know tokens, so a token cannot reach them. This is deliberate:
those endpoints do not check scopes, and a token should not be a way around that.

- `identify_api` (in `facets/core/src/access/identity.rs`) resolves a bearer token and no session cookie to its owner in
  its organization; a request with a session cookie is the browser's, whatever else it sends. Everything after that is
  unchanged: roles, record rules, field rules, capabilities, audit.
- **CSRF** (`facets/authentication/src/csrf.rs`): the double-submit and Origin checks are skipped only for those two paths,
  with a bearer token and no session cookie. A script cannot meet them, and such a request carries no credential the
  browser adds by itself, so there is nothing a forged cross-site request could ride on. A browser session that also
  sends a token is checked in full.
- **Audit:** the person is the actor (`actor_id`), and the call's `plugin_calls.user_agent` reads
  `token:<id>; <the client's user agent>`. Every data access of the request is tied to that call by the request id, so
  "who did this" and "through what" are both answered, and the chain ([Audit chain](audit_chain.md)) covers it.
- **Limits:** `api.rate_per_minute` (600) per token, counted per server process; failed attempts count against the
  client address like any other failed login; the last use and address are recorded at most once a minute.
- **Revoking** takes effect at once on the server that revokes; other servers reuse a lookup for up to 30 seconds. A
  deactivated owner ends their tokens the same way.

## Record API

A plain REST surface over the models a plugin **offers**, so nothing becomes reachable by accident. The plugin lists
them in `plugin.toml`:

```toml
[[api]]
model = "lead"
operations = ["list", "get", "create", "update", "delete"]   # default: ["list", "get"]
```

```
GET    /api/v1/{plugin}/{model}?filter=<url-encoded filter JSON>&fields=a,b&limit=50&after=<next>
POST   /api/v1/{plugin}/{model}              body: the fields            -> 201 { data }
GET    /api/v1/{plugin}/{model}/{id}                                    -> 200 { data }   (404 if absent or not readable)
PATCH  /api/v1/{plugin}/{model}/{id}         body: the fields to change  -> 200 { data }
DELETE /api/v1/{plugin}/{model}/{id}                                    -> 200 { deleted }
GET    /api/v1/{plugin}/openapi.json
```

- It calls the same `db::*` code as plugins do, **as the caller**: the model's record and field rules, workflow,
  `checks` and naming series apply, a refused write changes nothing, and a record the caller may not read is "not found"
  for a change or delete too. Offering a model in `[[api]]` is what allows the `db::query` / `db::mutate` commands; the
  rules still decide record by record. The plugin does not need to declare those capabilities for this.
- Fields go in and out **by name**; the stable ids never leave. A filter is the language of [Queries](queries.md). A field
  the caller may not read is not returned (and a filter on it is refused); one they may not write is refused with `403`.
- A list is ordered by id and paged with `after` (the previous page's `next`), so a record added meanwhile is never skipped
  or repeated. `limit` is 1 to 200 (default 50). `fields` picks columns (the `id` is always there).
- Errors are `{ "error": { "code", "message" } }` with the code `unauthenticated`, `denied`, `token_scope`, `not_found`,
  `invalid`, `rate_limited` or `internal`.
- A browser session works too, with its CSRF token. A visitor (anonymous) never does.
- **OpenAPI:** `GET /api/v1/{plugin}/openapi.json` is **generated** from the plugin's model files and `[[api]]` blocks, and
  describes what the *calling credential* can do: only the operations the plugin offers, the token's scopes and the
  person's grants leave; a field they cannot read is absent, one they cannot write is read-only; choices, limits and
  required fields come from the model. There is no second source, so it cannot drift from the behaviour. It is per plugin
  (the kernel does not load every plugin to describe them all).
- Every call is audited: a `plugin_calls` row named `api:<operation>:<model>`, and a `data_access` row for the records.

## Webhooks

A webhook tells an outside system that something happened. A plugin decides which of the events it emits may leave,
and an organization administrator decides where they go.

### The plugin opts an event in

```toml
[[events]]
name = "lead_won"
direction = "emit"
webhook = ["id", "title", "amount", "status"]   # the payload fields that may leave; nothing else ever does
```

An event with no `webhook` list cannot be sent by any webhook. When the plugin emits it (`events::emit`), the payload is
cut down to the listed top-level fields before anything is stored or sent. (The payload is written by the plugin, so the
list is the control; it is not worked out under anyone's record rules, and only organization administrators can
subscribe.)

### The administrator subscribes

An organization administrator (the `org_admin` role) manages subscriptions with `/api/ui/webhooks`:

```
GET    /api/ui/webhooks                        the subscriptions (never their secrets)
POST   /api/ui/webhooks  { name, url, events, conditions? }    -> 201 { id, secret }   (shown once)
PATCH  /api/ui/webhooks/{id}  { name?, url?, events?, conditions?, active? }
DELETE /api/ui/webhooks/{id}
POST   /api/ui/webhooks/{id}/rotate            -> { secret }   (the old one signs too for 24 hours)
POST   /api/ui/webhooks/{id}/test              sends an `aether.ping` event now, tried once
GET    /api/ui/webhooks/{id}/deliveries?limit=50
POST   /api/ui/webhooks/deliveries/{ulid}/replay
```

`events` are full names (`crm.lead_won`) of installed plugins, up to 50; `conditions` is `{ "status": "won" }`: every field
named must equal its value in what is sent (a field that is not sent never matches). At most 100 subscriptions. The secret
(`whsec_` and 48 hex characters) is stored encrypted ([secret settings](communication.md#secret-settings)).

### What is sent

A `POST` of this JSON, as exactly these bytes:

```json
{ "id": "<delivery id>", "event": "crm.lead_won", "created_at": "2026-10-17T10:00:00Z", "data": { "id": "...", "title": "..." } }
```

with the headers `X-Aether-Event`, `X-Aether-Delivery` (the same id), `X-Aether-Attempt` (1, 2, 3 ...) and

```
X-Aether-Signature: t=<unix seconds>,v1=<hex HMAC-SHA256 of "<t>.<body>" under the secret>[,v1=<under the old secret>]
```

A receiver recomputes the HMAC over `t`, a dot and the raw body, compares it with any `v1` in constant time, and refuses a
`t` that is too old. Two `v1` appear during the day after a rotation, so the receiver can switch secrets without losing events.

### How it is sent

- **A job.** Each matching subscription gets a delivery row and a scheduler job of kind `webhook` (queue `webhooks`), so it is
  sent off the request that emitted the event, at least once, and survives a restart. Webhooks are not held back by the depth
  limit that stops event handlers from looping, because nothing inside answers them.
- **Retries.** A failure (no answer, or anything but a 2xx) is tried again by the queue: `webhooks.max_attempts` in all (8),
  the wait starting at `webhooks.retry_base_secs` (60) and doubling. Then the delivery is `dead` and an administrator can send
  it again. A subscription whose deliveries give up `webhooks.disable_after` times in a row (20) is switched off, with the
  reason, until an administrator switches it on.
- **The sender is guarded.** Only `https://` unless `webhooks.allow_http`; no credentials in the address; the name is resolved
  by the kernel and refused unless **every** address is public (no loopback, private, link-local or metadata addresses),
  unless its host is listed in `webhooks.allowed_hosts`; the connection goes to the address that was checked, so the name
  cannot change underneath; no redirects; no proxy; 10 seconds; a body of at most 256 KiB (larger data is recorded as `dead`).
- **The log.** Every delivery is kept (data sent, status, attempts, the receiver's status and the first 1 KiB of its answer, the
  duration) for `webhooks.log_days` (14). A resend is a new delivery with `replay_of` set.

### Inbound hooks

An outside system (a payment provider, a form service) tells a plugin something by calling an address the plugin declares:

```toml
[[inbound]]
name = "payments"
function = "on_payment"          # the plugin function that gets the call
verify = "hmac-sha256"           # how the caller proves who it is: "aether", "hmac-sha256" or "token"
header = "x-signature"           # the header that carries the signature or the key
```

The address is `POST /api/hooks/<organization>/<plugin>/<name>`. The organization is in the address because the caller is
a machine with no session and no knowledge of how the instance finds organizations; it is the database name (`tok_org`).
It works only after an organization administrator has made the hook's secret:

```
GET    /api/ui/inbound/<plugin>                  the plugin's hooks: how verified, the address, whether a secret is set, counters
POST   /api/ui/inbound/<plugin>/<name>/secret    make the secret (201), or rotate it (200); shown once; the old one works for 24 hours
PATCH  /api/ui/inbound/<plugin>/<name>  { enabled }
DELETE /api/ui/inbound/<plugin>/<name>           remove the secret and the counters
```

A call is checked **before any plugin code runs**:

| `verify` | The caller sends | Checked |
|---|---|---|
| `aether` | `X-Aether-Signature: t=<unix>,v1=<hex>` (the form our own webhooks use) | HMAC-SHA256 of `<t>.<body>`, and `t` within `tolerance_secs` (300) of now, so a captured call cannot be replayed later |
| `hmac-sha256` | the named header: the hex HMAC-SHA256 of the raw body, optionally after `sha256=` | the same HMAC under the secret |
| `token` | the named header: the secret itself | equal to the secret |

Comparisons are in constant time, and with `aether` the signature is checked before the time, so a stranger learns nothing
about the clock from a wrong secret. During the day after a rotation both secrets are accepted.

- **Nothing to probe.** A hook that does not exist, is not installed, has no secret or is switched off answers `404` with the
  same body, whoever asks, for an organization or plugin that does not exist too. A call that fails the check answers
  `401 {"error":"invalid signature"}`; the reason goes to the log and to the hook's counters, never to the caller.
- **No browser rules.** The route needs no session and has no CSRF check (other systems cannot pass them), and it never looks
  at cookies: a signed-in browser without the signature is refused like anyone else. It is the only route besides the token
  endpoints that is exempt, and `/api/ui/inbound/...` (managing hooks) is not.
- **Limits.** The body is at most 1 MiB (`413` beyond, without reading the rest); calls count against the anonymous per-address
  request budget.
- **What the plugin gets.** The function runs as the kernel (`hook:<plugin>.<name>`), under the plugin's own capabilities, and
  is audited like any plugin call (with the caller's address). Its argument is
  `{ hook, body, raw, content_type, headers, query, received_at, remote_ip }`: `body` is the parsed JSON (or null), `raw` the
  text; `headers` leaves out cookies, credentials and anything named like a signature, key, secret or token. What the
  function returns (an object) is the answer. A failure answers `500` ("the hook failed") with none of the plugin's words,
  or `422` when it can never work, so the sender knows whether to try again.
- **Synchronous.** The call waits for the function (a plugin call is limited to 10 seconds). The sender may retry, so the
  function should be safe to repeat; a function that needs more time should queue its own work with `scheduler::enqueue`.

## Commands

None for plugins yet: tokens, the record API and webhooks are kernel HTTP endpoints. Proposed, so a plugin can manage its
own subscriptions: `webhooks::subscribe`, `unsubscribe`, `list` and `deliveries` (capability `webhooks::manage`).

## Settings

Built: `api.enabled` (on), `api.token_max_days` (365), `api.rate_per_minute` (600) in `seeds/settings/api.json`;
`webhooks.allow_http` (off), `webhooks.allowed_hosts` (none), `webhooks.max_attempts` (8), `webhooks.retry_base_secs` (60),
`webhooks.log_days` (14), `webhooks.disable_after` (20) in `seeds/settings/webhooks.json`. Organizations can override each.
Inbound hooks have no settings: the declaration is the plugin's, the secret the administrator's.

## Tables

`api_tokens` (core database, migration `047_api_tokens.surql`); `webhook_subscriptions` and `webhook_deliveries`
(organization databases, migration `037_webhooks.surql`); `inbound_hooks` (migration `038_inbound_hooks.surql`).

## Where it lives

`facets/core/src/access/api_token.rs` (tokens, scopes, lookup, limits), `access/identity.rs` (`identify_api`),
`api_tokens_api.rs` (the endpoints), `plugin_manager/record_api.rs` and `openapi.rs` (the record API and its
description), `plugin_manager/api.rs` (`prepare_call`, shared with plugin calls, and the scope check on
`/api/plugins/...`), `facets/authentication/src/csrf.rs` (the exemption), the `[[api]]` block in
`plugin_manager/models/plugin_def.rs`; `webhooks.rs` (signing, projection, the sender and the delivery job, the log
cleanup), `webhooks_api.rs` (the administrator endpoints), `plugin_events.rs` (where an emitted event is queued for
webhooks), `scheduler/worker.rs` (the `webhook` job kind), `webhook` in `[[events]]`; `inbound.rs` (the receiver, the three
verifications, the administrator endpoints), `[[inbound]]` in `plugin_def.rs`, `plugin_manager/api.rs` (`run_system_call_from`,
which records the caller's address), the `/api/hooks/` exemption in `csrf.rs`. End-to-end checks over HTTP:
`tests/e2e/api_tokens/`.

## Not in scope

GraphQL, long-lived streaming APIs, per-field OAuth scopes for third-party apps (a later "act on behalf of" grant
could build on tokens).

## Build order

1. Tokens and bearer identity, with CSRF rule and audit fields. **Built.**
2. Record API and generated OpenAPI. **Built.**
3. Webhooks out (job kind, signing, log). **Built.**
4. Inbound hooks. **Built.**

## Decide

| # | Question | Proposal |
|---|---|---|
| 1 | Token scope model | **Owner's rights narrowed by scopes**, never wider. |
| 2 | Must models opt in to the record API? | **Yes**, `[[api]]` per model. |
| 3 | Is the OpenAPI document generated or written? | **Generated** from models, rules and `[[api]]`, per plugin (built). |
| 6 | Which endpoints accept a token? | **Only `/api/plugins/...` and `/api/v1/...`** (built): the rest do not check scopes, so they do not know tokens. |
| 7 | How does the audit say "through a token"? | **In the call's `user_agent` column** (`token:<id>; ...`), tied to the data accesses by the request id (built): no change to the audit tables, which the [audit chain](audit_chain.md) hashes. |
| 4 | Webhook payload built as whom? | **Exactly the fields the emitting plugin lists in `webhook = [..]`**, nothing else (built). The first proposal said "as the subscriber, under their rules"; an event payload is written by the plugin and is not a record read, so the plugin's list is the control, and only administrators can subscribe. |
| 5 | Block private addresses for webhook URLs? | **Yes**, with an explicit allow-list setting (`webhooks.allowed_hosts`) for on-premises use (built). |
| 9 | Where is the organization in an inbound address? | **In the path** (`/api/hooks/<organization>/<plugin>/<name>`), not worked out from the request: the caller is a machine and any tenancy mode must work (built). |
| 10 | Queue the work and answer at once, or wait? | **Wait** (synchronous) for now: simpler, and the sender learns the outcome; queuing is a later option (built as synchronous). |
| 8 | Who retries? | **The scheduler's own retries** (a delivery is a job that fails until it works or runs out of attempts), not a second mechanism (built). |
