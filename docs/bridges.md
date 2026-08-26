## Aether Bridges

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
