## Aether Bridges

Bridges are first-party integrations compiled into the kernel. A bridge's settings and credentials live in the database settings (global, with an organization override), secrets encrypted; nothing about a bridge is written in `aether.toml`. Implemented so far: the messaging bridges (SMTP, Resend, Twilio, Termii, Africa's Talking; see [Communication](architecture/communication.md)), **Paystack** and **OpenStreetMap**, which plugins call with `bridge::call`. The table below is the plan; the rest are still empty crates.

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
| OpenSearch | Search and vector search; general purpose, also used by [deduplication](architecture/deduplication.md#opensearch-a-bridge-any-plugin-can-use) |
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


## Calling a bridge: `bridge::call`

Messaging bridges are driven by the kernel (`communication::send`). The others are called by plugins:

```toml
# plugin.toml
capabilities = ["bridge::call"]
bridges = ["paystack"]            # the bridges this plugin may call
```

```rhai
let started = bridge::invoke("paystack", "initialize_transaction", #{ email: input.email, amount: 50000, currency: "NGN" });
// started = #{ authorization_url: "...", access_code: "...", reference: "..." }
let result = bridge::invoke("paystack", "verify_transaction", #{ reference: started.reference });
```

```rust
let started: Value = bridge::call("paystack", "initialize_transaction", &json!({ "email": email, "amount": 50000 }))?;
```

(`call` is a reserved word in Rhai, so scripts say `bridge::invoke`.)

* The plugin needs the capability `bridge::call` **and** the bridge in its `bridges` list; a manifest naming
  an unknown bridge is refused when it is loaded.
* **Credentials are the organization's, and never reach the plugin.** The bridge's settings
  (`bridge.<name>.<field>`) are read from the database for the organization of the call, with the usual
  rules: an organization's own account if it filled in any of the account fields, otherwise the global one,
  never a mix; secrets are decrypted only inside the kernel. A missing setting is an error naming it.
* The call is **synchronous**: the plugin waits for the answer, at most 45 seconds. An error carries the
  provider's own message, and a provider's rejection (bad key, bad request) is not worth retrying while a
  timeout or a 5xx may pass; retrying is the plugin's decision (or the plugin enqueues a job).
* Logged with the plugin, bridge, action and duration, never the parameters or the answer.
* A bridge returns only the fields it documents, not the provider's whole answer.

| Bridge | Settings | Actions |
|---|---|---|
| `paystack` | `secret_key` (secret), `base_url` | `initialize_transaction { email, amount, currency?, reference?, callback_url?, metadata? }` (amount in the currency's smallest unit) returns `{ authorization_url, access_code, reference }`; `verify_transaction { reference }` returns `{ status, reference, amount, currency, paid_at, channel, customer_email, gateway_response }` |
| `open_street_map` | `base_url`, `contact` | `geocode { query, limit?, country_codes? }` returns a list of `{ display_name, lat, lon, kind, importance }`; `reverse { lat, lon }` returns `{ display_name, lat, lon, address }` or `null`. The public server allows about one request a second and asks for a contact: requests are spaced a second apart and identify the kernel; for real volume run your own Nominatim and set `base_url` |

### Adding a bridge

Implement `ActionBridge` (in the `communication` facet) in a crate under `bridges/`, describe its settings
in a `Spec` and its actions in `ACTIONS`, list it in `facets/core/src/bridges.rs`, and add a group to
`seeds/settings/bridges.json`. Tests check that the seed and the bridge agree and that secret fields are
marked secret.
