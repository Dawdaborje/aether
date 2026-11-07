---
title: "Aether Bridges"
description: "Bridges are first-party integrations compiled into the kernel. A bridge's settings and credentials live in the database settings (global, with an organization override), secrets encrypted; nothing…"
section: guide
status: stable
order: 60
---
## Aether Bridges

Bridges are first-party integrations compiled into the kernel. A bridge's settings and credentials live in the database settings (global, with an organization override), secrets encrypted; nothing about a bridge is written in `aether.toml`. Implemented so far: the messaging bridges (SMTP, Resend, Twilio, Termii, Africa's Talking; see [Communication](architecture/communication.md)), **Paystack**, **OpenStreetMap**, **Jitsi Meet** (meeting links), **Collabora Online** (office editing) **JMAP** and **IMAP** (a person's own mailbox), which plugins call with `bridge::call`. The table below is the plan; the rest are still empty crates.

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
| `liteparse` *(internal: used by `ocr::parse`, not by `bridge::call`)* | `binary_path`, `default_language`, `tessdata_path`, `ocr_server_url`, `ocr_server_token` (secret), `dpi`, `max_pages`, `timeout_secs` | `parse { path, language?, pages?, ocr?, max_pages?, max_text_bytes?, timeout_secs? }`, `status {}`; see [Document parsing](architecture/ocr.md) |
| `ollama` *(internal: used by `ai::complete`, not by `bridge::call`)* | `base_url`, `default_model`, `default_embed_model` | `chat { messages, model?, max_tokens?, temperature?, json?, timeout_secs? }`, `embed { input, model? }`, `models {}`; local models, see [AI](architecture/ai.md) |
| `openai_compatible` *(internal)* | `api_key` (secret), `base_url`, `default_model`, `default_embed_model` | `chat`, `embed`, `models`; OpenAI, OpenRouter, vLLM, LM Studio and any server with the same API |
| `anthropic` *(internal)* | `api_key` (secret), `base_url`, `default_model` | `chat`, `models`; no embeddings |
| `jitsi` | `base_url`, `app_id`, `signing_key` (secret) | `join_link { room, name?, email?, moderator?, ttl_secs? }` returns `{ url, room, signed }`. Calls no server: it builds the address of a room on a Jitsi Meet server. With the server's token authentication set (`app_id` and `signing_key`) the address carries a signed token for one room, one person and a few hours, and says whether they moderate; without them it is the plain room address (a public server) |
| `collabora` | `base_url`, `wopi_host_url`, `signing_key` (secret) | `editor_url { media, name, user, user_name?, can_write?, plugin, save, ref, ttl_secs? }` returns `{ url, expires }`: the address that opens one file for one person in the editor server (Collabora Online or another WOPI editor). The editor reads and saves the file through the kernel's WOPI endpoints (see below) with the access token in the address |
| `jmap` *(works for a person)* | `session_url` | `check`, `mailboxes`, `list { mailbox?, search?, limit?, position? }`, `get { id }`, `mark { id, read }`, `trash { id }`, `send { to, cc?, subject, text, in_reply_to?, references? }` on **the calling person's own** mailbox on a JMAP mail server (see below) |
| `imap` *(works for a person)* | `host`, `port`, `security` (`tls`, `starttls`, `none`), `smtp_host`, `smtp_port`, `smtp_security`, `domain` | The same actions as `jmap`, for a mail server that speaks IMAP. Reading goes over IMAP; sending goes over SMTP with the person's own login, and a copy is kept in the Sent folder. A login that is not an address becomes `login@domain`. A message id is `<uid>.<folder>`; the text is read from the MIME message (plain text, or HTML with its tags removed), attachments are decoded, nothing remote is loaded |
| `open_street_map` | `base_url`, `contact` | `geocode { query, limit?, country_codes? }` returns a list of `{ display_name, lat, lon, kind, importance }`; `reverse { lat, lon }` returns `{ display_name, lat, lon, address }` or `null`. The public server allows about one request a second and asks for a contact: requests are spaced a second apart and identify the kernel; for real volume run your own Nominatim and set `base_url` |

### Bridges that work for a person

Some services are not the organization's but a person's own: their mailbox. The bridge list `bridges::PER_USER` (`jmap`, `imap`) marks them. A person
connects their login once (`PUT /api/ui/bridge-credentials/<bridge>` with `{ username, secret }`, or the `<login bridge="jmap"/>` page
element); the kernel checks it with the service, stores the secret encrypted in the organization's database and never shows it again
(`GET` answers only whether and as whom they are connected, `DELETE` forgets it). When a plugin calls such a bridge the kernel looks up **the
caller's** login and gives it to the bridge: the plugin never sees it and cannot name another person. With no login connected the call is
refused with a message the person can act on.

### Office editing: the kernel as a WOPI host

`collabora` makes the address; the kernel answers the editor's calls without a session, by the access token in the address (signed with the
bridge's `signing_key`, naming one file and one person, good for hours):

```text
GET   /api/wopi/files/<file>?access_token=       CheckFileInfo
GET   /api/wopi/files/<file>/contents            GetFile
POST  /api/wopi/files/<file>/contents            PutFile
```

A save is never written over the old file. The new bytes get a new media key and the plugin named in the token (`save`) is called **as the
person**, with `{ file: <ref>, media: { key, name, mime, size } }`; the drive makes a new version under its own rules and the quota. If it
refuses, the new bytes are removed. No locks are offered. The editor server must be allowed to call this server (its WOPI host list) and the
setting `wopi_host_url` must be the address it reaches this server at.

### Adding a bridge

Implement `ActionBridge` (in the `communication` facet) in a crate under `bridges/`, describe its settings
in a `Spec` and its actions in `ACTIONS`, list it in `facets/core/src/bridges.rs`, and add a group to
`seeds/settings/bridges.json`. Tests check that the seed and the bridge agree and that secret fields are
marked secret.
