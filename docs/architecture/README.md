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

| Doc | What it covers |
|---|---|
| [Mantle](mantle.md) | Working-set cache engine: labels, heat, warm-set tiers, desk inspector |
| [Caching layer](cache.md) | Plugin-facing `cache::*`, Moka / Redis backends |
| [Logging](logging.md) | Process log level, stderr vs `[logging]` file under `app_dir`, CLI overrides |
| [Gaps roadmap](gaps_roadmap.md) | Index and build order of the proposals below (proposal) |
| [History](history.md) | Record versions, point-in-time reads, undo, restore (proposal) |
| [Audit chain](audit_chain.md) | Hash-chained, signed, verifiable audit trail (proposal) |
| [API and webhooks](api_and_webhooks.md) | API tokens, record API with generated OpenAPI, webhooks (proposal) |
| [Import and export](import_export.md) | CSV/XLSX importer with dry run and undo (proposal) |
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
