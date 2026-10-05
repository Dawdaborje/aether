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
