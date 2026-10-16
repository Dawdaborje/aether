---
title: "API tokens, record API and webhooks"
description: "Today the only credential is the browser session cookie (aether_session), and the only public entry to plugin code is POST /api/plugins/{plugin}/{function}."
section: architecture
status: proposal
order: 40
---
# API tokens, record API and webhooks (proposal)

**Status: proposal, nothing built.** Choices are marked *Decide* in the last section.

Today the only credential is the browser session cookie (`aether_session`), and the only public entry to plugin
code is `POST /api/plugins/{plugin}/{function}`. A script, another system or a mobile client has no way in, and
nothing tells another system that something happened. This page adds three things on the same rails: **API
tokens**, a **record API with a generated OpenAPI description**, and **webhooks**.

## API tokens

A token is a credential for a *person or service account* in one organization, limited on purpose.

```
POST /api/auth/tokens  { name, expires_at?, scopes: { plugins: ["crm"], models: ["lead:read,write"], functions: ["crm.add_lead"] } }
-> { id, token: "aet_..." }   // shown once
```

- Stored as a SHA-256 hash with a prefix for display; the plain token is never recoverable.
- Sent as `Authorization: Bearer aet_...`. `identify()` in `facets/core/src/access/identity.rs` learns this second
  source; everything after it is unchanged: roles, record rules, field rules, capabilities, audit.
- A token **can never exceed its owner**: rules are evaluated for the owner's roles, then narrowed by the scopes.
  Removing a role from the user takes effect on the next call.
- The audit row names the token (`actor_type: token`, `actor_id: user`, `token_id`), so "who did this" and "through
  what" are both answered.
- CSRF: the double-submit check in `facets/authentication/src/csrf.rs` is skipped **only** for requests that carry a
  valid bearer token and no session cookie. A request with both is treated as cookie-authenticated.
- Limits: per-token rate limit, optional IP allow-list, expiry (default 90 days, maximum configurable), revoke at any time.
  Last-used time and IP are recorded.

## Record API

A generic REST surface over models a plugin has **opted in to expose**, so nothing becomes public by accident:

```toml
# plugin.toml
[[api]]
model = "lead"
operations = ["list", "get", "create", "update", "delete"]
```

```
GET    /api/v1/{plugin}/{model}?filter=<url-encoded filter JSON>&fields=a,b&limit=50&after=<cursor>
GET    /api/v1/{plugin}/{model}/{id}
POST   /api/v1/{plugin}/{model}          PATCH /{id}        DELETE /{id}
```

- It calls the same `db::*` code as plugins do, so the filter language is [Queries](queries.md), pagination is
  cursor-based, and rules and workflow apply (a refused write changes nothing).
- Fields appear by **name** in and out; the stable id never leaks to clients.
- `GET /api/v1/openapi.json` is **generated** from model definitions, rules and the `[[api]]` blocks. It lists only
  what the calling token can reach, with field types, enums, required flags and limits taken from the model. The
  description cannot drift from behavior because there is no second source.
- Idempotency: `Idempotency-Key` header on writes (24 h memory).
- Errors use one JSON shape with a stable code.

## Webhooks

A plugin declares what it publishes; an organization subscribes where it wants it sent.

```toml
[[webhooks]]
name = "lead_won"
event = "crm.lead_won"            # an event from events.md
payload = ["id", "name", "amount"] # fields sent; nothing else
```

Subscriptions live in organization data (created in a settings page or by `webhooks::subscribe`), each with a URL,
a secret, a chosen set of events and optional filter (same filter language, evaluated before sending).

- **Delivery** is a scheduler job of kind `webhook` (new arm in `scheduler/worker.rs::execute`), at least once,
  exponential retries up to a limit, then marked failed and visible.
- **Signing**: `X-Aether-Signature: t=<unix>,v1=HMAC-SHA256(secret, t.body)`; receivers reject old `t`. The secret
  is stored with `secrets.rs` and shown once; a rotation overlaps the old and new for 24 h.
- **Safety of the sender**: the URL is resolved at send time and refused if it is a private, loopback or link-local
  address (guards against using the server to reach internal services), HTTPS only unless allowed per org,
  10 s timeout, 256 KiB body cap, no redirects followed.
- **Log and replay**: every attempt is kept (status, duration, truncated body, response head) for
  `webhooks.log_days`; an administrator can resend one.
- Payload is built **as the subscription's owner** under their rules, so a webhook cannot leak a field its
  subscriber could not read.

### Inbound

`POST /api/hooks/{plugin}/{name}` for services that call Aether (a payment provider, a form tool). The plugin declares
the route and the **verification method** (HMAC header, shared secret in a header, or a provider signature handled
by a bridge); the kernel verifies before any plugin code runs, then calls the declared function with the body.
Unverified calls are refused and counted. These routes are exempt from CSRF and from login, and nothing else is.

## Commands

| Command | Capability | Does |
|---|---|---|
| `webhooks::subscribe` / `unsubscribe` / `list` | `webhooks::manage` | manage subscriptions of the plugin's events |
| `webhooks::deliveries` | `webhooks::manage` | recent attempts |
| `api::tokens` (kernel API, not a plugin command) | | create and revoke tokens for the signed-in user |

## Settings

`api.enabled`, `api.token_max_days`, `api.rate_per_minute`, `webhooks.allow_http`, `webhooks.max_attempts`,
`webhooks.log_days`, `webhooks.allowed_hosts` (optional allow-list).

## Tables

`api_tokens` (hash, prefix, owner, scopes, expires, last_used), `webhook_subscriptions`, `webhook_deliveries`.

## Where it lives

`facets/authentication` (token issue and check), `facets/core/src/access/identity.rs` (bearer source),
`facets/core/src/routes.rs` (a new `api_v1.rs` merged beside the others), `scheduler/worker.rs` (job kind),
`capabilities/webhooks.json`, SDK `../sdks/rust/src/webhooks.rs`.

## Not in scope

GraphQL, long-lived streaming APIs, per-field OAuth scopes for third-party apps (a later "act on behalf of" grant
could build on tokens).

## Build order

1. Tokens and bearer identity, with CSRF rule and audit fields.
2. Record API and generated OpenAPI.
3. Webhooks out (job kind, signing, log).
4. Inbound hooks.

## Decide

| # | Question | Proposal |
|---|---|---|
| 1 | Token scope model | **Owner's rights narrowed by scopes**, never wider. |
| 2 | Must models opt in to the record API? | **Yes**, `[[api]]` per model. |
| 3 | Is the OpenAPI document generated or written? | **Generated** from models, rules and `[[api]]`. |
| 4 | Webhook payload built as whom? | **The subscriber**, under their rules. |
| 5 | Block private addresses for webhook URLs? | **Yes**, with an explicit allow-list setting for on-premises use. |
