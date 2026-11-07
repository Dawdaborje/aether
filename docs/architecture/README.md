---
title: "Architecture Overview"
description: "How the SvelteKit frontend, the Rust kernel, the WebAssembly plugin host and SurrealDB fit together, with links to every architecture topic."
section: architecture
status: stable
order: 10
---
## Architecture Overview

```
┌─────────────────────────────────────────────────────┐
│                   SvelteKit Frontend                 │
│   Dynamic renderer · Plugin UI · Theme tokens       │
└────────────────────────┬────────────────────────────┘
                         │ REST / SSE      
┌────────────────────────▼────────────────────────────┐
│                    Aether Kernel (Rust)              │
│                                                     │
│  ┌──────────┐  ┌──────────┐  ┌───────────────────┐  │
│  │  Router  │  │  Facets  │  │   Bridge Registry │  │
│  │  (Axum)  │  │          │  │  Stripe · Paystack│  │
│  └────┬─────┘  └────┬─────┘  │  Mayan · Resend   │  │
│       │             │        └───────────────────┘  │
│  ┌────▼─────────────▼──────────────────────────┐    │
│  │              WASM Host (Extism)            │    │
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

## Topics

These pages document the kernel and the platform features plugins build on. What a single plugin or suite does (its models, roles, roadmap) is documented in that plugin's own `docs/` folder under `plugins/`.

| Doc | What it covers |
|---|---|
| [Mantle](mantle.md) | Working-set cache engine: labels, heat, warm-set tiers, desk inspector |
| [System health](system_health.md) | The desk page and API that show whether the database, scheduler, plugin runtime and cache are working |
| [Caching layer](cache.md) | Plugin-facing `cache::*`, Moka / Redis backends |
| [Logging](logging.md) | Process log level, stderr vs `[logging]` file under `app_dir`, CLI overrides |
| [Gaps roadmap](gaps_roadmap.md) | Index and build order of the proposals below (proposal) |
| [History](history.md) | Record versions, point-in-time reads, undo, restore, redaction, retention |
| [Uninstalling plugins](uninstall.md) | Uninstall keeping data, orphaned data, purge with backup, `on_uninstall` |
| [Audit chain](audit_chain.md) | Hash-chained, signed, verifiable audit trail |
| [API and webhooks](api_and_webhooks.md) | API tokens, the record API with generated OpenAPI and outgoing webhooks and inbound hooks (built) |
| [Import and export](import_export.md) | CSV and XLSX importer with dry run, upsert keys and undo; CSV and JSON export (built) |
| [Inbound mail](inbound_mail.md) | Email into records and chatter (proposal) |
| [Auth hardening](auth_hardening.md) | Second factor, passkeys, SAML, per-organization policy (proposal) |
| [Field encryption](field_encryption.md) | Per-field encryption with customer-held keys (proposal) |
| [Sharing](sharing.md) | Share a record with a person or link, with expiry (proposal) |
| [Automation](automation.md) | Rules that act, assignment, repeats, approvals (proposal) |
| [Verification](verification.md) | Static checks, previews on a copy, upgrade pre-flight (proposal) |
| [Print](print.md) | Print templates and PDF (proposal) |
| [Views and reports](views_reports.md) | Pivot, graph, gantt, map, tree; saved reports (proposal) |
| [Customization](customization.md) | Per-organization overlays and sandboxed hooks (proposal) |
| [Federation](federation.md) | Signed, consented exchange between organizations (proposal) |
| [Realtime](realtime.md) | WebSocket with topics, presence and typing for chat-like apps |
| [Kernel improvements](kernel_improvements.md) | Review of the kernel and the work to do, in build order (partly built) |
| [Theme breaking points](theme_gaps.md) | Where theme plugins break today and the engine changes to fix them (proposal) |
| [Custom pages](custom_pages.md) | Panes, live data and plugin layouts (partly built) |
| [Universal document viewer](document_viewer.md) | One viewer for every file, plugin viewers in sandboxed JavaScript (partly built) |
| [Guided tours](guided_tours.md) | Plugin-declared walkthroughs, with optional video steps, shown on first visit and remembered per person (proposal) |
| [Plugin author tutorial](plugin_tutorial.md) | A build-it-yourself path from empty folder to a tested plugin (proposal) |
