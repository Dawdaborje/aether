# Aether

> The word *Aether* (also spelled *Aither* or *Ether*) comes from Greek mythology — the personification of the upper sky, the pure light the gods breathed. Everything in Aether passes through the kernel the same way light passes through the ether.

Aether is a modular, plugin-driven Business Suite framework built on a Rust kernel with WebAssembly plugins, a SurrealDB multi-model database, and a SvelteKit frontend. It is a **personal learning project** — built to explore systems programming, compiler design, capability-based security, and plugin architecture. Licensed MIT.

> Aether is not trying to beat Odoo. Odoo has 20 years, hundreds of engineers, and millions of lines of code. Aether exists to learn by building something real and complex.

---

## Why Aether

| Concern | Aether's answer |
|---|---|
| Plugin isolation | WebAssembly sandboxing + capability system |
| Multi-tenancy | SurrealDB namespace/database isolation per org |
| Plugin language | Any language that compiles to WASM (Rust, Python, Go, TypeScript) and support for rhai and lua |
| UI extensibility | XML DSL (`.xml`) compiled to a SvelteKit component tree at build time |
| Performance | Rust kernel, compiled WASM modules cached at startup |
| Deployment | Single binary, minimal infrastructure |
| Developer experience | CLI toolchain, typed SDK per language, live dev server |
| Storage | Multiple supported storage (Local file storage, s3 storage and more)

---

## Terminology

| Term | Description |
|---|---|
| **Kernel** | The Rust server and WebAssembly runtime host. Manages plugin lifecycle, routing, capability enforcement, and database access. |
| **Plugin** | A WebAssembly module that implements the Aether Extism interface. Can be written in Rust, Python, Go, TypeScript, or any WASM-targeting language. |
| **Facet** | A core module built into the kernel — Auth, Tenancy, Scheduler, Cache, Audit Log. Always available, never reimplemented by plugins. |
| **Bridge** | A first-party integration with a third-party service — Stripe, Paystack, Mayan EDMS, Resend, Africa's Talking. Compiled into the kernel as optional Cargo features. |
| **Capability** | A named permission a plugin declares it needs — `db::query`, `email::send`, `storage::write`. The kernel enforces these at runtime. |
| **DSL** | An XML-based UI definition language (`.xml` files) that compiles to a SvelteKit component tree. Plugin authors define pages declaratively without writing frontend code. |

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
| Plugin interface | Extism, Rhai, Lua |


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

## Contributing

Aether is an open learning project. If you find it interesting, contributions are welcome.

Read [CONTRIBUTING.md](./CONTRIBUTING.md) for setup instructions, code style, and how to write your first plugin.

Good first contributions:
- Implement a new Bridge (Twilio, Flutterwave, Africa's Talking)
- Add a new built-in `.xml` DSL tag
- Write a sample plugin in Python or Go
- Improve CLI error messages
- Write documentation

---

## License

MIT — see [LICENSE](./LICENSE).