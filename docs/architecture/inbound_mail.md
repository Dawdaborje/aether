---
title: "Inbound mail"
description: "The kernel sends mail (communication::send, Communication); it never receives it. Support tickets, leads and replies on a record all start as an email somewhere."
section: architecture
status: proposal
order: 220
---
# Inbound mail (proposal)

**Status: proposal, nothing built.** Choices are marked *Decide* in the last section.

The kernel sends mail (`communication::send`, [Communication](communication.md)); it never receives it. Support
tickets, leads and replies on a record all start as an email somewhere. This page turns mail into records and
chatter messages without adding any mail code to plugins.

## Two ways in

1. **Provider webhook**: the mail provider posts each received message to `/api/inbound/{bridge}`. Fast, no polling,
   needs a provider that supports it.
2. **IMAP poll**: a scheduled job reads a mailbox with a stored login. Works with any mailbox, a minute or so late.

Both end in the same place: a normalized `InboundMessage` and a **routing** step. A bridge implements a new trait
next to `EmailBridge` in `facets/communication/src/lib.rs`:

```rust
pub trait InboundBridge { fn verify(&self, req: &Request) -> Result<Vec<InboundMessage>, SendError>; }
// IMAP: fn poll(&self, since: Cursor) -> Result<(Vec<InboundMessage>, Cursor), SendError>
```

The webhook route is one of the few unauthenticated routes; the bridge's `verify` (signature, token) runs before any
parsing, and nothing is stored from an unverified call.

## Routing

An **alias** is an address (or a plus-tag) that belongs to one plugin function:

```toml
# plugin.toml
[[mail_routes]]
alias = "support"                   # support@<org mail domain>
function = "helpdesk.on_mail"
model = "ticket"                    # new records land here
thread = true                       # replies attach to the record they belong to
```

Order of matching: (1) a **reply**: `In-Reply-To`/`References` or the record token in the address
(`support+tk_8f3a@...`) finds the record and appends to its chatter; (2) an **alias**: calls the route's
function with the message, which creates or updates a record; (3) otherwise the message goes to the **unmatched
queue** for an administrator, never silently dropped and never guessed.

The function receives a clean structure, not raw MIME:

```json
{ "from": {"name","address"}, "to": [], "cc": [], "subject": "", "text": "", "html_sanitized": "",
  "attachments": [{"media_key","name","size","type"}], "message_id": "", "in_reply_to": "", "received_at": "" }
```

Attachments are stored through `storage::*` first (size and type limits apply); HTML is sanitized by the kernel.
The function runs **as the sender's matching user** when the address belongs to one, otherwise as the route's
service role, which is declared in the plugin and visible to administrators.

## Safety

- Loops: auto-replies, `Auto-Submitted`, `Precedence: bulk` and mailing-list headers are detected and not routed
  (or routed with a flag). A sender sending more than N per hour to one alias is throttled.
- Spoofing: SPF/DKIM results from the provider are passed on as `auth: { spf, dkim }`; a route can require a pass.
- Size and count limits per message; archives are never unpacked by the kernel.
- Duplicates: `message_id` is remembered per alias, so a provider retry creates nothing twice.

## Commands

| Command | Capability | Does |
|---|---|---|
| `mail::routes` | `mail::manage` | list the aliases a plugin declares and their state |
| `mail::reply { record, body, as }` | `mail::send` | send a reply that keeps the thread headers and record token |
| `mail::unmatched` / `mail::assign` | `mail::manage` | the unmatched queue and a manual routing |

Outbound replies reuse `communication::send`; this page only adds the thread headers.

## Settings

`mail.inbound.provider`, `mail.inbound.domain`, `mail.imap.*` (host, port, user, secret password), `mail.max_bytes`,
`mail.poll_secs`, `mail.unmatched_keep_days`. Credentials are secret settings as in
[Communication](communication.md#secret-settings).

## Tables

`inbound_messages` (message id, route, status, error, record link, kept per `mail.keep_days`), `inbound_cursors`
(IMAP position per mailbox).

## Not in scope

Sending as a user from their own mailbox, calendar invitations, an in-app mail client, spam filtering beyond
header checks (the provider's job).

## Build order

1. The trait, normalization and the webhook route with one provider bridge.
2. Routing, alias declaration, unmatched queue, chatter append.
3. IMAP polling bridge.
4. Loop and spoof protection, throttles.

## Decide

| # | Question | Proposal |
|---|---|---|
| 1 | Webhook or IMAP first? | **Webhook first**; IMAP follows for self-hosters. |
| 2 | Run the route as the sender or as a service role? | **Sender when known**, otherwise a declared service role. |
| 3 | Unroutable mail | **Unmatched queue**, never dropped. |
| 4 | Where is HTML cleaned? | **In the kernel**, once, before any plugin sees it. |
