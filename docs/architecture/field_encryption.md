---
title: "Field encryption with customer-held keys"
description: "Today one installation key (facets/core/src/secrets.rs, AES-256-GCM, enc:v1: values) protects secret settings. Business data in model fields is stored in the clear in the database."
section: architecture
status: proposal
order: 170
---
# Field encryption with customer-held keys (proposal)

**Status: proposal, nothing built.** Choices are marked *Decide* in the last section.

Today one installation key (`facets/core/src/secrets.rs`, AES-256-GCM, `enc:v1:` values) protects secret
*settings*. Business data in model fields is stored in the clear in the database. Some organizations need to say:
"even the operator of this server cannot read these fields without my key, and if I withdraw the key the data is
gone". This page designs that, field by field.

## Declaring it

```json
{ "name": "salary", "type": "decimal", "encrypted": { "search": "none" } }
{ "name": "national_id", "type": "string", "encrypted": { "search": "equality" } }
```

`search` is `none` (default) or `equality`. No other query ability exists for an encrypted field (see below).

## Key hierarchy

```
KEK  (key-encryption key)   held by the customer: a KMS key, or a passphrase-derived key
 └─ DEK (data key), one per organization and key version, stored wrapped by the KEK
     └─ field value:  enc:v2:<key_id>:<b64(nonce || ciphertext)>
```

- Values are encrypted with the DEK, authenticated with the **record id, field id and model id as associated
  data**, so ciphertext cannot be copied to another record or field.
- The DEK is held in memory only while needed and unwrapped through the KEK. It is cached with a time limit
  (`encryption.dek_cache_secs`); after that the kernel must ask the KEK again.
- KEK sources, through a new bridge category `bridges/key_management/`: `local` (the installation key, the
  default, no external dependency), `kms` (cloud KMS wrap/unwrap), `vault`, and `passphrase` (the owner types it
  at start-up or unlock). A customer-managed key means the operator can host the server but cannot decrypt after the
  customer disables the key.
- The `v2` header carries `key_id`, so many key versions coexist and **rotation** is: add a new DEK version, new
  writes use it, a background job re-encrypts old values (a job with a callback, resumable, audited).

## What can and cannot be done with an encrypted field

| Operation | Support |
|---|---|
| Read, write, display, import, export | Yes, under the same field read/write roles ([Rules](rules.md)) |
| Equality filter (`search: equality`) | Yes, with a **blind index**: HMAC-SHA256 of the normalized value under an index key derived from the DEK, stored beside the field |
| Unique, upsert key, link target | Only with `equality` |
| Sort, range, `like`, aggregate, full-text | **No.** The model loader refuses a query, index or report that needs them, at load time |
| Calculated field depending on it | Allowed; the result is encrypted if any input is |
| Chatter tracking | Allowed; the stored before/after is encrypted too |

The blind index leaks equality (two equal values are visibly equal) and nothing else. That is stated here so a
model author chooses knowingly. Low-cardinality fields (a yes/no) should not use `equality`.

## Where it is applied

In the data layer, once: `facets/core/src/data_model/runtime.rs` (name-to-id translation and value checks) encrypts
on the way in and decrypts on the way out, after the field rules decide the caller may see it. Plugins, the
record API and exports never see ciphertext unless they ask for `raw`. Version rows ([History](history.md)),
exports and backups store ciphertext. Logs and the audit trail never contain values.

## Crypto-shredding

`encryption::destroy_key { key_id }` (owner only, recent second factor, confirmation, audited) removes the wrapped
DEK. Every value under it, in every version row and every backup, becomes unreadable. This is the supported
answer to "delete this person's data everywhere", and it is irreversible. A KEK withdrawn at the customer's KMS has
the same effect without any command to the kernel.

## Failure behavior

If the KEK is unreachable: reads of encrypted fields fail with `key_unavailable` (a clear error; the rest of the
record loads without those fields where the role allows partial reads), writes to them are refused. Nothing is
silently written in plain text. A health check shows the state.

## Commands and settings

| Command | Capability | Does |
|---|---|---|
| `encryption::status` | `encryption::read` | key sources, versions, last rotation |
| `encryption::rotate` | `encryption::manage` | start a re-encryption job |
| `encryption::destroy_key` | `encryption::manage` | crypto-shred |

Settings: `encryption.kek_source`, `encryption.dek_cache_secs`, `encryption.rotate_batch`. Credentials for a
KMS bridge are secret settings per organization.

## Tables

`org_keys` (key_id, version, wrapped DEK, kek reference, state: active, retiring, destroyed), a hidden
blind-index column beside each searchable encrypted field.

## Not in scope

End-to-end encryption in the browser (the server must still compute with values it holds), encrypting whole
files in object storage with the same scheme (a later step, same DEK hierarchy), homomorphic or order-preserving schemes.

## Build order

1. `v2` format, key table, `local` KEK, read/write path for `search: none`.
2. Blind index and `equality`.
3. Rotation job and `destroy_key`.
4. KMS and passphrase bridges.
5. Model-loader checks (refuse unsupported queries and indexes).

## Decide

| # | Question | Proposal |
|---|---|---|
| 1 | Granularity | **Per field**, not per model or per organization only. |
| 2 | Search on encrypted fields | **Equality via blind index only**; everything else refused at load. |
| 3 | Default KEK | **Local** (works anywhere); KMS and passphrase optional. |
| 4 | Authenticated data | **Model, field and record id**, so ciphertext cannot be moved. |
| 5 | If the key is unavailable | **Fail closed** with `key_unavailable`. |
