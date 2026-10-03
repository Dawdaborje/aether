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
| Data | `db::get`, `db::find`, `db::create`, `db::update`, `db::delete` | `db::query`, `db::mutate` |
| Notifications | `notify::send` | `notify::send`, `notify::public` |
| Live events | `events::emit` | `events::emit` |
| Who is calling | `context::get` | none |

There is no raw SurQL: a plugin names a model it was granted, and the kernel builds the query,
scoped to the organization and audited. Cache, storage, email, SMS, HTTP, plugin-to-plugin,
bridge and scheduler commands are reserved in the capability catalog but not implemented yet.

Rust, for example:

```rust
use aether_sdk::prelude::*;

Message::find().filter("channel", "general").limit(20).all()?;
Notification::new("New message").link("/chat").to(Audience::Members).send()?;
events::emit("typing", &json!({ "who": "ann" }))?;
```
