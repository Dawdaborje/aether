## SDKs

Plugin authors do not touch Extism or the kernel's JSON protocol: each language gets an SDK that
turns the kernel's commands into ordinary functions. The SDKs live outside this repository, in
`sdks/<language>` next to it.

| Language | State |
|---|---|
| Rust | Built: `sdks/rust` (see its README). |
| Go, TypeScript, JavaScript, Python | Not started. |

What the kernel offers a plugin today, and so what an SDK wraps:

| Area | Commands | Capability |
|---|---|---|
| Data | `db::get`, `db::find`, `db::create`, `db::update`, `db::delete`, `db::increment` | `db::query`, `db::mutate` |
| Data (atomic) | `db::transaction` | `db::transaction`, `db::mutate` |
| Cache | `cache::get`, `cache::set`, `cache::invalidate`, `cache::clear` | same names |
| Files | `storage::read`, `storage::write`, `storage::delete`, `storage::list` | same names |
| Web APIs | `http::request` | `http::request` (+ `http_hosts`) |
| Other plugins | `plugins::call` | `plugins::call` (+ `dependencies`) |
| Files | `fs::read`, `fs::write`, `fs::list`, `fs::stat`, `fs::rename`, `fs::delete` (and `[[watch]]` in the manifest) | `fs::read`, `fs::write`, `fs::list`, `fs::delete` |
| Integrations | `bridge::call` | `bridge::call` (+ `bridges` in `plugin.toml`) |
| Events between plugins | `events::emit`, `events::subscribe`, `events::unsubscribe` | `events::emit`, `events::subscribe` |
| Messages | `communication::send` (one command; the type says email, SMS, …) | `communication::send` + `email::send` / `sms::send` |
| Background work | `scheduler::enqueue`, `scheduler::job`, `scheduler::cancel_job`, `scheduler::register`, `scheduler::cancel` | `scheduler::enqueue`, `scheduler::register`, `scheduler::cancel` |
| Notifications | `notify::send` | `notify::send`, `notify::public` |
| Live events | `events::emit` | `events::emit` |
| Who is calling | `context::get` | none |

There is no raw SurQL: a plugin names a model it was granted, and the kernel builds the query,
scoped to the organization and audited. Bridge and event-subscription
commands are reserved in the capability catalog but not implemented yet. [Kernel
commands](plugin/commands.md) lists every payload, limit and status; SDKs for Go, TypeScript,
JavaScript and Python need to wrap the cache, storage, HTTP, plugin and transaction commands too.

Rust, for example:

```rust
use aether_sdk::prelude::*;

Message::find().filter("channel", "general").limit(20).all()?;
Notification::new("New message").link("/chat").to(Audience::Members).send()?;
events::emit("typing", &json!({ "who": "ann" }))?;
```
