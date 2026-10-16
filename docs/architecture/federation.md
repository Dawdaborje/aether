---
title: "Federation: governed exchange between organizations"
description: "A supplier sends an invoice to a customer; a customer sends an order to a supplier. Today both sides retype it, or one of them gets an email attachment."
section: architecture
status: proposal
order: 160
---
# Federation: governed exchange between organizations (proposal)

**Status: proposal, nothing built.** Choices are marked *Decide* in the last section.

A supplier sends an invoice to a customer; a customer sends an order to a supplier. Today both sides retype it, or one of
them gets an email attachment. Both companies may be organizations on the same Aether installation, or on different
ones. This page designs an exchange where each side **consents**, the content is **signed**, and **both** sides keep an
audit of it, without either organization reading the other's data.

## Principles

1. **Push, never pull.** An organization sends a document it chose to send. The other organization's data is never
   queried.
2. **Consent first.** Nothing flows between two organizations until they have an active **link** both accepted.
3. **A contract per document type.** The shape of what may be sent is a versioned schema both sides support.
4. **Evidence on both sides.** Each side stores the signed envelope.

## Link

```
federation_links: id, local_org, peer (org id on this installation, or a peer address + public key),
                  state (invited | active | paused | revoked), accepted_by_local, accepted_by_peer,
                  document_types [..], created_at
```

An administrator invites a peer; the peer's administrator accepts, with a [recent second factor](auth_hardening.md) on
both sides. Either side may pause or revoke at any time; revocation stops new documents at once and keeps the evidence.
Which document types flow is part of the link.

## Document contract

A plugin declares what it can send and receive:

```toml
[[federation]]
type = "invoice"            # type name; versioned: invoice@1
model = "invoice"           # local model it maps from / to
send = "federation/invoice.send.json"   # mapping: local fields -> contract fields
receive = "federation/invoice.receive.json"  # contract fields -> local record, and which function creates it
```

The contract schema is a JSON Schema checked into the plugin (`federation/schemas/invoice@1.json`). The kernel validates
every outgoing and incoming document against it. Mappings are data, so they can be checked
([Verification](verification.md)). Versions negotiate: the link records the highest version both support.

## Envelope

```json
{ "id": "...", "type": "invoice@1", "from": "orgA", "to": "orgB", "sent_at": "...",
  "payload": { ... }, "attachments": [ { "name": "INV-2026-001.pdf", "sha256": "..." } ],
  "prev": "hash of the previous envelope in this link", "signature": "Ed25519 over the canonical envelope" }
```

- Signed with the sending organization's federation key (generated per organization; public key shown on the link
  and rotatable; same key custody options as the [audit chain](audit_chain.md)).
- Attachments (typically the archived PDF from [Print](print.md)) are referenced by hash and sent with the envelope.
- `prev` chains the envelopes in one link, so a missing document is detectable, the same device as the audit chain.

## Delivery

- **Same installation**: the kernel places the envelope in the peer's inbox table and wakes a job; no network.
- **Different installations**: HTTPS `POST /api/federation/inbox` to the peer, verified by signature against the link's
  stored key, retried like [webhooks](api_and_webhooks.md#webhooks) with the same address protections and idempotency.
- The receiving kernel validates, stores the **raw signed envelope** in its own evidence table, and then calls the
  plugin's `receive` function **as a service role declared for that link**, which creates a *draft* record; a person confirms
  (or an [automation](automation.md) accepts, by explicit choice). Receiving never overwrites existing records
  by itself.
- The sender gets a signed **receipt** (`received`, then `accepted` or `rejected` with a reason), recorded in both audits.

## Rights and privacy

- The sender builds the payload **as the sending user**, under their rules; the mapping lists the fields, so nothing
  outside it can leave.
- The receiver trusts the signature, not the content: validation against the contract and the receiving organization's
  own `checks` still apply.
- Encrypted fields ([Field encryption](field_encryption.md)) are decrypted by the sender only for fields the mapping names,
  and the transfer is TLS.
- Neither side can query or list the other's records; the only things visible to a peer are the envelopes it received and
  the receipts.

## Commands

| Command | Capability | Does |
|---|---|---|
| `federation::links` | `federation::read` | links and their state |
| `federation::send { link, type, record }` | `federation::send` | build, sign, deliver |
| `federation::inbox { link }` | `federation::receive` | received envelopes and states |
| `federation::answer { envelope, accept \| reject, reason }` | `federation::receive` | receipt |

## Settings and tables

Settings: `federation.enabled`, `federation.max_attachment_bytes`, `federation.allowed_peers` (cross-installation allow-list),
`federation.inbox_keep_days`. Tables: `federation_links`, `federation_keys`, `federation_outbox`, `federation_inbox`,
`federation_receipts`.

## Not in scope

A public directory of organizations, payments settlement, document formats of national e-invoicing networks (an
envelope type plus a bridge can carry them), shared editing of one record by two organizations.

## Build order

1. Same-installation links, envelopes, inbox, receipts.
2. Contract schemas, mappings, drafts and confirmation.
3. Cross-installation delivery with signatures and retries.
4. Key rotation, pause/revoke, evidence export.

## Decide

| # | Question | Proposal |
|---|---|---|
| 1 | Push or pull | **Push only.** |
| 2 | Received documents | **Draft record, a person or an explicit automation accepts.** |
| 3 | Do both sides sign receipts | **Yes**; evidence on each side. |
| 4 | Start with same-installation | **Yes**, cross-installation after. |
| 5 | Contract format | **JSON Schema per `type@version`**, shipped by the plugin. |
