# Kernel commands

A plugin has no database, file system or network of its own. Everything it does outside its own
code is a **kernel command**: a named request such as `db::create` that the kernel checks and then
carries out. WebAssembly plugins send them through their SDK; [Rhai scripts](rhai.md) call them as
functions (`db::create(...)`). Both reach the same code, so the same checks apply.

## How a command is checked

1. **Capability.** The plugin lists what it needs in `plugin.toml` (`capabilities = ["db::query",
   "cache::get"]`). A command whose capability is missing is refused, and a manifest naming a
   capability that does not exist is rejected when the plugin is loaded. Anonymous visitors get
   only the capabilities in `public_capabilities`.
2. **Scope.** Each command is limited to what the plugin was given: models in `access_models`,
   its own cache entries and files, the hosts in `http_hosts`, the plugins in `dependencies`.
3. **Audit.** Database commands leave a `data_access` row; plugin calls leave a call record.

The same payloads are used by WASM and Rhai; an answer is `{ "ok": true, "data": ... }`, and a
refusal is an error with a message (a script can `try`/`catch` it).

## Reference

| Command | Capability | Rhai | Status |
|---|---|---|---|
| `db::get`, `db::find`, `db::query` | `db::query` | `db::get`, `db::find` | done |
| `db::create`, `db::update`, `db::delete`, `db::increment` | `db::mutate` | same names | done |
| `db::mutate` | `db::mutate` | use `db::create/update/delete` | done |
| `db::transaction` | `db::transaction` (+ `db::mutate`) | `db::transaction` | done |
| `cache::get`, `cache::set`, `cache::invalidate`, `cache::clear` | same name | `cache::get/set/invalidate/invalidate_prefix/clear` | done |
| `storage::read`, `storage::write`, `storage::delete`, `storage::list` | same name | `storage::read/read_base64/write/write_base64/delete/list` | done |
| `http::request` | `http::request` (+ `http_hosts`) | `http::request/get/post` | done |
| `plugins::call` | `plugins::call` (+ `dependencies`) | `plugins::invoke` | done |
| `notify::send` | `notify::send`, `notify::public` | `notify::send` | done |
| `events::emit` | `events::emit` | `events::emit` | done |
| `context::get` | none | `context::get` | done |
| `communication::send` | `communication::send` + `email::send` / `sms::send` | `communication::send` | done: one command for every message type, see [Communication](../architecture/communication.md) |
| `scheduler::enqueue`, `scheduler::job`, `scheduler::cancel_job` | `scheduler::enqueue` | `scheduler::enqueue/job/cancel_job` | done: background jobs, see [the scheduler](../architecture/scheduler.md) |
| `scheduler::register`, `scheduler::cancel` | same name | `scheduler::register/cancel` | done: recurring tasks (also `[[schedule]]` in `plugin.toml`) |
| `bridge::call` | `bridge::call` | not bound | **not implemented**: the other bridges (payments, storage, …) are still empty |
| `events::subscribe` | `events::subscribe` | not bound | **not implemented** |

A command that is not implemented checks its capability and then answers "not implemented".
`call` is a reserved word in Rhai, which is why `plugins::call` is `plugins::invoke` in scripts.

## db::transaction

Several writes, applied together or not at all. Each entry has the fields of the single command
plus `op` (`create`, `update`, `delete` or `increment`); 1 to 50 writes.

```json
{ "ops": [
  { "op": "create", "model": "invoice", "data": { "number": "A-1" } },
  { "op": "increment", "model": "counter", "id": "invoices", "field": "next" }
] }
```

Every write is checked against the plugin's model grants exactly as if sent alone, and each leaves
its own audit row (and chatter entry) inside the same transaction. If any write fails, none is
applied. `data` in the answer lists each write's record, in order.

```rhai
let made = db::transaction([
    #{ op: "create", model: "invoice", data: #{ number: "A-1" } },
    #{ op: "increment", model: "counter", id: "invoices", field: "next" },
]);
```

## cache

A small key/value store for a plugin's own use, shared between its calls. Values are JSON.

| | |
|---|---|
| `cache::set { key, value, ttl_secs? }` | store; `ttl_secs` is 1 to 30 days, otherwise the cache's default applies |
| `cache::get { key }` | the value, or `null` |
| `cache::invalidate { key }` or `{ prefix }` | remove one entry or every entry starting with `prefix`; answers `{ removed }` |
| `cache::clear {}` | remove all of **this plugin's** entries in this organization |

Entries are kept under the namespace `plugin:<organization>:<plugin>`: another plugin or
organization can neither read nor clear them. Keys are at most 256 bytes; a value must fit the
cache's `max_value_bytes`. The cache can drop entries at any time (memory pressure, a restart,
a different node when Redis is not used), so it holds things that can be recomputed, never the
only copy of data.

## storage

Files in the organization's media storage (a local directory or S3, whichever `[media]` selects).

| | |
|---|---|
| `storage::write { key, text }` or `{ key, base64 }` | write a file, replacing any existing one |
| `storage::read { key, encoding? }` | `{ text }`, or `{ base64 }` with `encoding: "base64"`; `null` when the file does not exist |
| `storage::delete { key }` | remove; a missing file is not an error |
| `storage::list { prefix? }` | `[ { key, size } ]`, at most 1000 |

A plugin's files live under `plugins/<plugin>/` inside the organization's folder; the kernel adds
the prefix, so a plugin cannot reach another plugin's files or the organization's uploads, and
keys with `..`, empty segments or backslashes are refused. A file is at most 10 MiB through this
command. Backend errors are logged and the plugin only sees "storage is unavailable".

## http::request

Calls an outside web API.

```json
{ "method": "POST", "url": "https://api.example.com/v1/charges",
  "headers": { "authorization": "Bearer ..." }, "json": { "amount": 500 }, "timeout_secs": 10 }
```

Give one body: `body` (text), `json` or `base64`. The answer is `{ status, ok, headers, body }`
(`body_base64` when the answer is not text); a `404` or `500` from the other side is an answer, not
an error. Restrictions, all enforced by the kernel:

- **Allowlist.** `plugin.toml` must list the host: `http_hosts = ["api.example.com",
  "*.example.org"]`. A wildcard covers subdomains, not the bare domain. Nothing listed means no
  access.
- **Public addresses only.** The host is resolved by the kernel and refused if any address is
  loopback, private (10/8, 172.16/12, 192.168/16), link-local (including the 169.254.169.254
  cloud metadata address), carrier-grade NAT, multicast or otherwise reserved, for IPv4 and IPv6.
  The connection is made to the checked address, so DNS cannot change the answer afterwards.
- **No redirects** are followed (a redirect could leave the allowlist); the `3xx` answer and its
  `location` header are returned.
- `https` only, no username or password in the URL, methods GET, POST, PUT, PATCH, DELETE and
  HEAD, request and response bodies of at most 1 MiB, a timeout of 10 s by default (30 s at most),
  no proxy. Headers the kernel sets itself (`host`, `content-length`, `connection`, ...) are refused.
- Every call is logged with the plugin and host (never the URL's query or the headers).

```rhai
let answer = http::get("https://api.example.com/rate");
let created = http::post("https://api.example.com/charges", #{ amount: 500 });   // a map is sent as JSON
let full = http::request(#{ method: "PUT", url: "...", headers: #{ "x-key": "..." }, body: "..." });
if full.status >= 400 { fail("the payment service refused it"); }
```

## plugins::call

One plugin calls a function of another and gets its JSON result (`plugins::invoke` in Rhai).

```rhai
let rate = plugins::invoke("currency", "convert", #{ amount: 10, to: "EUR" });
```

- The target must be in the caller's `dependencies` (`plugin.toml`), which also tells the
  administrator what must be installed.
- The target runs **as the same actor** as the request, under **its own** capabilities and model
  grants. Nothing is lent in either direction: calling `billing` does not give the caller access
  to billing's models, and an anonymous visitor still reaches only the target's `public_functions`.
- It must be installed and enabled in the organization, like any call.
- Calls nest at most 4 deep and a function cannot be called while it is already running further up
  the chain, so plugins cannot loop. Waiting for a free execution slot is bounded (10 s), after
  which the call fails with "busy".
- The target's `fail("...")` message comes back as the error. Each nested call is recorded in the
  plugin call audit under the same request id.

## communication::send

One command for email, SMS and anything else the kernel learns to send. The plugin gives the
`type` and the content and never a provider or a sender; the kernel queues one job per recipient and
the scheduler delivers it through the configured bridge, with retries.

```json
{ "type": "email", "to": ["ann@example.com"], "subject": "Your invoice", "text": "Total: 500" }
{ "type": "sms",   "to": "+2348012345678", "text": "Your code is 1234" }
```

Needs `communication::send` **and** `email::send` or `sms::send`. Answer: `{ jobs: [ids] }`. A field the
kernel does not know, such as `from`, is refused. Full rules, limits, providers and settings are in
[Communication](../architecture/communication.md).

```rhai
let queued = communication::send(#{ type: "sms", to: input.phone, text: "Your code is " + code });
```

## scheduler::*

Background jobs (`scheduler::enqueue { function, payload?, delay_secs?, queue?, max_attempts?,
unique_key? }`, `scheduler::job { id }`, `scheduler::cancel_job { id }`) and recurring tasks
(`scheduler::register { name, function, cron | every, … }`, `scheduler::cancel { name }`). Jobs run
one of the plugin's own functions, as the kernel, with the plugin's own permissions, at least once.
Tasks that always exist are declared as `[[schedule]]` in `plugin.toml`. See
[the scheduler](../architecture/scheduler.md).

```rhai
scheduler::enqueue("send_report", #{ month: "2026-10" }, #{ delay_secs: 60, unique_key: "report-2026-10" });
```

## Not implemented yet

| Command | What is missing |
|---|---|
| `bridge::call` | Bridges other than messaging: payments, storage, documents, … (their crates under `bridges/` are empty). |
| `events::subscribe` | Plugin-to-plugin event delivery. `[[events]]` with `direction = "listen"` is parsed but nothing dispatches to it. |
| Mailgun, Postmark, Bravo, WhatsApp | Messaging providers whose crates are still empty; the kernel routes only to SMTP, Resend, Twilio, Termii and Africa's Talking. |
