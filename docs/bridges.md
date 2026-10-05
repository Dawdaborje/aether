## Aether Bridges

Bridges are first-party integrations compiled into the kernel. A bridge's settings and credentials live in the database settings (global, with an organization override), secrets encrypted; nothing about a bridge is written in `aether.toml`. Only the messaging bridges are implemented so far (SMTP, Resend, Twilio, Termii, Africa's Talking): see [Communication](architecture/communication.md). The table below is the plan; the others are still empty crates.

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

A plugin does not call a messaging bridge by name either: it sends a message of some type with
`communication::send` and the kernel routes it to the one the settings select.
