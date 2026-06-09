# Aether

> The word *Aether* (also spelled *Aither* or *Ether*) comes from Greek mythology — the personification of the upper sky, the pure light the gods breathed. Everything in Aether passes through the kernel the same way light passes through the ether.

Aether is a modular, plugin-driven ERP framework built on a Rust kernel with WebAssembly plugins, a SurrealDB multi-model database, and a SvelteKit frontend. It is a **personal learning project** — built to explore systems programming, compiler design, capability-based security, and plugin architecture. Licensed MIT.

> Aether is not trying to beat Odoo. Odoo has 20 years, hundreds of engineers, and millions of lines of code. Aether exists to learn by building something real and complex.

---

## Why Aether

| Concern | Aether's answer |
|---|---|
| Plugin isolation | WebAssembly sandboxing + capability system |
| Multi-tenancy | SurrealDB namespace/database isolation per org |
| Plugin language | Any language that compiles to WASM (Rust, Python, Go, TypeScript) |
| UI extensibility | XML DSL compiled to a SvelteKit component tree at build time |
| Performance | Rust kernel, compiled WASM modules cached at startup |
| Deployment | Single binary, minimal infrastructure |
| Developer experience | CLI toolchain, typed SDK per language, live dev server |

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────┐
│                   SvelteKit Frontend                 │
│   Dynamic renderer · Plugin UI · Theme tokens       │
└────────────────────────┬────────────────────────────┘
                         │ REST / WebSocket
┌────────────────────────▼────────────────────────────┐
│                    Aether Kernel (Rust)              │
│                                                     │
│  ┌──────────┐  ┌──────────┐  ┌───────────────────┐  │
│  │  Router  │  │  Facets  │  │   Bridge Registry │  │
│  │  (Axum)  │  │          │  │  Stripe · Paystack│  │
│  └────┬─────┘  └────┬─────┘  │  Mayan · Resend   │  │
│       │             │        └───────────────────┘  │
│  ┌────▼─────────────▼──────────────────────────┐    │
│  │              WASM Host (Wasmtime)            │    │
│  │  Capability gating · Per-request instances  │    │
│  │  Kernel commands · Cross-plugin events       │    │
│  └────────────────────┬─────────────────────────┘   │
└───────────────────────┼─────────────────────────────┘
                        │
          ┌─────────────▼─────────────┐
          │        SurrealDB           │
          │  NS: aether               │
          │  DB: core  (kernel data)  │
          │  DB: org_* (tenant data)  │
          └───────────────────────────┘
```

Read the full [Architecture Document](./docs/architecture/README.md).

---

## Terminology

| Term | Description |
|---|---|
| **Kernel** | The Rust server and WebAssembly runtime host. Manages plugin lifecycle, routing, capability enforcement, and database access. |
| **Plugin** | A WebAssembly module that implements the Aether WIT interface. Can be written in Rust, Python, Go, TypeScript, or any WASM-targeting language. |
| **Facet** | A core module built into the kernel — Auth, Tenancy, Scheduler, Cache, Audit Log. Always available, never reimplemented by plugins. |
| **Bridge** | A first-party integration with a third-party service — Stripe, Paystack, Mayan EDMS, Resend, Africa's Talking. Compiled into the kernel as optional Cargo features. |
| **Capability** | A named permission a plugin declares it needs — `db::query`, `email::send`, `storage::write`. The kernel enforces these at runtime. |
| **DSL** | An XML-based UI definition language (`.ae` files) that compiles to a SvelteKit component tree. Plugin authors define pages declaratively without writing frontend code. |

---

## Stack

| Layer | Technology |
|---|---|
| Kernel | Rust · Axum · Tokio |
| WASM Runtime | Wasmtime (Component Model) |
| Database | SurrealDB |
| Object Storage | Garage (S3-compatible) |
| Frontend | SvelteKit · TypeScript |
| CLI | Rust (same workspace) |
| Plugin interface | WIT (WebAssembly Interface Types) |


## Plugin System

Plugins are WebAssembly modules. They implement the Aether WIT interface and communicate with the kernel exclusively through typed kernel commands.

```
Plugin calls Db.query()
  → kernel capability check
  → kernel scopes query to org automatically
  → kernel queries SurrealDB
  → result returned to plugin

Plugin never touches SurrealDB directly.
Plugin never touches the filesystem.
Plugin never makes raw network calls.
```

### Writing a Plugin

Every plugin declares its capabilities, routes, pages, and events in `plugin.toml`:

```toml
[module]
name    = "accounting"
version = "0.1.0"

[capabilities]
required = ["db::query", "db::mutate"]
optional = ["email::send", "storage::read"]

[[routes]]
path    = "/invoices"
method  = "GET"
handler = "list_invoices"

[[tasks]]
name     = "daily_report"
type     = "cron"
schedule = "0 8 * * *"

[[events]]
name    = "invoice.posted"
handler = "on_invoice_posted"
```

UI pages are defined in XML files:

```xml
<page route="/accounting/invoices" model="accounting.invoice" title="Invoices">
  <view type="list">
    <columns>
      <column field="number"       label="Invoice #"  sortable />
      <column field="partner"      label="Customer"   type="record_link" />
      <column field="amount_total" label="Total"      type="currency" />
      <column field="status"       label="Status"     type="badge" />
    </columns>
    <actions>
      <action name="post"   label="Post"   confirm />
      <action name="cancel" label="Cancel" danger />
    </actions>
  </view>
</page>
```

The `ae build` CLI compiles `.ae` files into a component tree JSON and WASM into a single distributable archive.

---

## SDK

Each language has a clean SDK that wraps the raw WIT bindings. Plugin authors never touch WIT directly.

**Rust**
```rust
use aether_sdk::{Db, Email, Events, Plugins};

Db::query::<Invoice>("invoice", "status = 'draft'")?;
Email::send("user@example.com", "Hello", "Body")?;
Events::emit("invoice.posted", &payload);
Plugins::call::<_, StockLevel>("inventory", "check-stock", &req)?;
```

**Python**
```python
from aether import Db, Email, Events, Plugins

Db.query("invoice", "status = 'draft'")
Email.send("user@example.com", "Hello", "Body")
Events.emit("invoice.posted", {"id": invoice_id})
Plugins.call("inventory", "check-stock", {"invoice_id": id})
```

**TypeScript**
```typescript
import { Db, Email, Events, Plugins } from '@aether/sdk';

await Db.query<Invoice>('invoice', "status = 'draft'");
Email.send('user@example.com', 'Hello', 'Body');
Events.emit('invoice.posted', { id: invoiceId });
Plugins.call<CheckRequest, StockLevel>('inventory', 'check-stock', req);
```

**Go**
```go
import "github.com/aether/sdk-go/db"
import "github.com/aether/sdk-go/events"

db.Query[Invoice]("invoice", "status = 'draft'")
events.Emit("invoice.posted", payload)
```

---

## Bridges

Bridges are first-party integrations compiled into the kernel as optional Cargo features. Enabled per-org, credentials stored encrypted in SurrealDB.

| Bridge | Category |
|---|---|
| Stripe | Payments (global) |
| Paystack | Payments (West Africa) |
| Flutterwave | Payments (Pan-African) |
| Africa's Talking | SMS (Africa) |
| Resend | Transactional email |
| Brevo | Email / SMTP |
| Mayan EDMS | Document management |
| Nextcloud | File storage |
| Keycloak | Identity provider |
| DHIS2 | Health data (Africa) |
| OpenIMIS | Health insurance |

Plugins call bridges through kernel commands — never directly:

```rust
// Plugin never imports Stripe SDK
// Kernel handles credentials internally
kernel_command("stripe::charge", payload)
```

Enable bridges in `aether.toml`:

```toml
[bridges]
stripe   = { enabled = true }
paystack = { enabled = true }
resend   = { enabled = true }
mayan    = { enabled = true, url = "https://mayan.internal" }
```

---

## Multi-Tenancy

Each organisation lives in its own SurrealDB database. Data isolation is structural, not policy-based.

```
Namespace: aether
  ├── Database: core       ← kernel config, plugin registry, user identities
  ├── Database: org_acme   ← acme's settings, installed plugins, business data
  └── Database: org_demo   ← demo's settings, installed plugins, business data
```

Users live in `core` and can belong to multiple organisations via a graph relation. Permissions are stored per-org.

---


## Roadmap

- [ ] Kernel — Axum router, WASM host, SurrealDB integration
- [ ] CLI — `init`, `add`, `install`, `build`, `dev`
- [ ] WIT interfaces — all core commands
- [ ] SDK — Rust, Python, TypeScript, Go
- [ ] DSL compiler — `.ae` XML → component tree JSON
- [ ] SvelteKit renderer — dynamic component tree rendering
- [ ] Multi-tenancy — namespace/database isolation
- [ ] Auth — JWT, user/org graph, permissions
- [ ] Facets — Scheduler, Audit Log, Cache, Rate Limiting
- [ ] Bridges — Resend, Stripe, Paystack, Mayan
- [ ] Plugin registry
- [ ] Documentation site

---

## Contributing

Aether is an open learning project. If you find it interesting, contributions are welcome.

Read [CONTRIBUTING.md](./CONTRIBUTING.md) for setup instructions, code style, and how to write your first plugin.

Good first contributions:
- Implement a new Bridge (Twilio, Flutterwave, Africa's Talking)
- Add a new built-in `.ae` DSL tag
- Write a sample plugin in Python or Go
- Improve CLI error messages
- Write documentation

---

## License

MIT — see [LICENSE](./LICENSE).

---

> Built by [@Dawdaborje](https://github.com/Dawdaborje) in The Gambia.