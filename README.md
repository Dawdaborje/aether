# Aether

A plugin-driven business suite framework. A Rust kernel hosts sandboxed plugins (WebAssembly, Lua, Rhai) on top of a multi-tenant SurrealDB database, and a SvelteKit frontend renders pages that plugins describe in XML.

Aether is a personal project for exploring systems programming, capability-based security and plugin architecture. The name comes from the Greek *aither*, the pure upper sky: everything in the system passes through the kernel the way light passes through it.

## Highlights

- **Sandboxed plugins.** Plugins run as WebAssembly (Extism), Lua (Luau) or Rhai, and can call each other across languages. They never touch the database, filesystem or network directly.
- **Capability enforcement.** A plugin declares the capabilities it needs (`db::query`, `email::send`, `storage::write`). The kernel checks them on every call.
- **Multi-tenancy.** Each organization gets its own SurrealDB namespace and database, and the kernel scopes every query automatically.
- **Declarative UI.** Pages are written in an XML DSL and rendered by the SvelteKit frontend, so plugin authors don't write frontend code.
- **Single binary.** The server, CLI and documentation site ship as one `aether` executable.
- **Pluggable storage.** Local disk, S3-compatible storage and others.
- **Built-in integrations.** Bridges for Stripe, Paystack, Mayan EDMS, Resend and Africa's Talking, compiled in as optional Cargo features.

## How a plugin call works

```
plugin calls Db.query()
  -> kernel checks the plugin's declared capabilities
  -> kernel scopes the query to the caller's organization
  -> kernel runs it against SurrealDB
  -> result returns to the plugin
```

## Concepts

| Term | Meaning |
|---|---|
| Kernel | The Rust server and WebAssembly runtime. Handles plugin lifecycle, routing, capability enforcement and database access. |
| Plugin | A unit of functionality written in Rust, Python, Go, TypeScript or any other language that targets WASM, or in Lua or Rhai. |
| Facet | A core module built into the kernel: auth, tenancy, scheduler, cache, audit log. Plugins use facets and never reimplement them. |
| Bridge | A first-party integration with a third-party service. |
| Capability | A named permission a plugin must declare before the kernel lets it act. |
| DSL | The XML page definition language that plugins use to describe their UI. |

## Stack

| Layer | Technology |
|---|---|
| Kernel and CLI | Rust, Axum, Tokio |
| Plugin runtime | Extism (WASM), Lua, Rhai |
| Database | SurrealDB |
| Object storage | Garage (S3-compatible), local disk |
| Frontend | SvelteKit, TypeScript |

## Repository layout

| Path | Contents |
|---|---|
| `aether/` | The `aether` binary: server and CLI |
| `facets/` | Core kernel modules |
| `bridges/` | Third-party service integrations |
| `capabilities/` | Capability definitions |
| `web/` | SvelteKit frontend |
| `migrations/`, `seeds/` | Database schema and seed data |
| `docs/` | Kernel and plugin-authoring documentation |
| `tests/` | End-to-end and integration tests |

## Getting started

You need the `aether` binary and a running SurrealDB. The [Extism CLI](https://github.com/extism/cli) is only required if you build WASM plugins. See [docs/prerequisite.md](docs/prerequisite.md) for details and [docs/get_started.md](docs/get_started.md) for the first steps.

## Documentation

- [Documentation index](docs/README.md)
- [Architecture](docs/architecture/README.md)
- [Writing plugins](docs/plugin/README.md)
- [Bridges](docs/bridges.md)
- [MCP server](docs/mcp.md)

## License

Released under the [Business Source License 1.1](LICENSE).
