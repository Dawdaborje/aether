# Communication: email, SMS and other messages

A plugin sends any message with **one command**, `communication::send`. It says what *type* of
message it is and what is in it. The kernel sends it through whichever provider the administrator
configured for that type, with that organization's credentials. The plugin never names a provider,
never holds a key and never chooses the sender.

```rhai
communication::send(#{ type: "email", to: ["ann@example.com"], subject: "Your invoice", text: "Total: 500" });
communication::send(#{ type: "sms", to: "+2348012345678", text: "Your code is 1234" });
```

```rust
communication::email(["ann@example.com"]).subject("Your invoice").text("Total: 500").send()?;
communication::sms(["+2348012345678"]).text("Your code is 1234").send()?;
```

| | |
|---|---|
| Capabilities | `communication::send`, **and** the capability of the type: `email::send` or `sms::send`, so spending on SMS can be granted separately |
| Answer | `{ jobs: [id, …] }`: the message is **queued**, not yet delivered |
| Email fields | `to` (one address or a list), `subject`, `text` and/or `html`, `reply_to` |
| SMS fields | `to` (international form, `+2348012345678`; spaces and dashes are dropped), `text` (at most 1600 characters) |
| Limits | 1 to 50 recipients per call; 1000 messages per plugin per organization per hour; an email body of at most 512 KiB |

What the kernel does before it answers:

1. checks the capabilities and the message, and refuses anything it does not know. **`from`, `sender`
   and similar fields are refused** rather than ignored: the sender is the administrator's choice
   (settings below), so a plugin cannot send as someone else;
2. checks that the type is **set up** (a provider is chosen and its settings are complete) and says so
   if not, so a plugin finds out when it sends, not hours later;
3. queues **one job per recipient** on the queue named after the type (`email`, `sms`). Recipients
   never see each other, and if the provider fails halfway a retry only repeats the messages that did
   not go through.

Delivery then happens in the [scheduler](scheduler.md): it hands the message to the bridge, retries
if the provider is unreachable or busy, and stops at once if the provider rejects the message or
the credentials. Jobs are delivered at least once, so a crash at the wrong moment can send a message
twice.

## Providers (bridges)

A bridge is a small crate under `bridges/communications/` that knows how to talk to one provider.
The traits and message types are in the `communication` facet.

| Type | Bridge | Notes |
|---|---|---|
| email | `smtp` | any SMTP server; `starttls` (587), `tls` (465) or `none` (25) |
| email | `resend` | Resend's API |
| sms | `twilio` | |
| sms | `termii` | channels `generic`, `dnd`, `whatsapp`; the address of your account's API is a setting |
| sms | `africas_talking` | use the sandbox address to test |

Not yet implemented: Mailgun, Postmark, Bravo and WhatsApp (their crates are still empty), and message
types other than `email` and `sms`. A new bridge implements `EmailBridge` or `SmsBridge`, describes its
settings in a `Spec`, is listed in `messaging/mod.rs`, and gets a group in `seeds/settings/communications.json`
(a test checks that the seed and the bridge agree).

## Configuring it: settings, not `aether.toml`

Everything lives in the database settings, at two levels: **global** (the default for every
organization) and **organization** (an override). The groups are *Email*, *SMS* and one per provider.

| Setting | |
|---|---|
| `communications.email.provider` | `smtp` or `resend`; blank turns email off |
| `communications.email.from_address`, `communications.email.from_name` | who email is from |
| `communications.sms.provider` | `twilio`, `termii` or `africas_talking`; blank turns SMS off |
| `communications.sms.sender` | a sender name or number, for providers that take one per message |
| `bridge.<provider>.<field>` | that provider's credentials and options, e.g. `bridge.twilio.auth_token` |

**An organization uses the global settings unless it fills in its own.** The rules:

* The provider choice, the sender and the plain options fall back one setting at a time.
* **Credentials are resolved as a unit.** The fields that identify the provider *account* (keys,
  passwords, the number registered to the account) always come from one place together. If the
  organization fills in **any** of them, it uses **only** its own, and a missing one is an error that
  names it: its account is never mixed with the global one's. If it fills in none, it uses the global
  account. This stops, for example, an organization's own Twilio account SID being paired with the
  global auth token.
* A blank value counts as not filled in. Saving a blank secret for an organization removes the
  organization's value, handing it back to the global one.

## Secret settings

A setting marked `secret` in its seed (API keys, passwords) is:

* **encrypted in the database** with AES-256-GCM, a fresh random nonce each time;
* **never returned by the API**: the settings screens receive a blank value and `has_value: true`
  (a *secret* type with "set" or "not set"); to change it you enter a new value;
* decrypted only inside the kernel, when a bridge needs it.

The key is **not** in the database, so a leaked database does not leak credentials. It comes from:

1. the `AETHER_SECRET_KEY` environment variable, else
2. `secret_key` under `[security]` in `aether.toml`, else
3. `<app_dir>/conf/secret.key`, created with a random key (mode 0600) the first time it is needed.

Every Aether process that reads credentials (the server and any standalone scheduler) needs the same
key: share the `app_dir`, or set the same variable. Back the key up with the database; without it the
stored secrets cannot be read and have to be entered again. Changing the key has the same effect.
If no key can be found or created, secret settings cannot be saved (the API says so) and everything
else keeps working.

## Trying it without a provider

Termii and Africa's Talking take an address setting, so you can point them at a local test server;
a plugin's message arrives there exactly as the provider would receive it. The bridge tests do this
with a small fake server.
