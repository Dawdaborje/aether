---
title: "Audit chain: tamper-evident audit"
description: "The audit trail (Access) is written in the same transaction as the work, so a write without an audit row cannot happen."
section: architecture
status: proposal
order: 50
---
# Audit chain: tamper-evident audit (proposal)

**Status: proposal, nothing built.** Choices are marked *Decide* in the last section.

The audit trail ([Access](access.md#the-audit-trail)) is written in the same transaction as the work, so a
write without an audit row cannot happen. But the rows themselves are ordinary rows: an administrator with
database access can edit or delete them, and nobody could tell. This page makes any such change **detectable**,
by anyone, with only a public key.

It does not stop a database administrator from changing data. It guarantees they cannot do so silently.

## How it works

Each audit row gets two columns:

```
data_access: ..., seq (per organization), prev_hash, row_hash
row_hash = SHA-256( prev_hash || canonical(row without the two hash columns) )
```

`seq` and `prev_hash` come from a single counter row per organization, read and advanced inside the write's
own transaction (`run_audited` in `facets/core/src/kernel/host/db.rs`). Two writes cannot take the same
`seq`; the transaction retries on conflict. The same is done for `plugin_calls` and `page_visits`, in separate chains.

**Canonical form** is a fixed field order, UTF-8, no whitespace, timestamps in UTC with nanoseconds. It is
specified in this page so a verifier in any language can be written.

## Checkpoints

Every N rows or every T minutes (`audit.checkpoint_rows`, `audit.checkpoint_secs`), the scheduler writes a
**checkpoint**:

```
audit_checkpoints: org, chain, from_seq, to_seq, head_hash, at, signature, key_id
signature = Ed25519( org || chain || to_seq || head_hash || at )
```

The signing key is the installation's audit key (generated at first start, stored beside the secret key with
mode 0600, rotatable; old public keys stay in `audit_keys` so old checkpoints still verify). With a KMS bridge
([Field encryption](field_encryption.md)) the key can live outside the server, so a server compromise cannot forge
checkpoints.

## Anchors

A checkpoint can be **exported** (`aether audit export-anchor`), a small JSON file, to somewhere the server's
administrator cannot rewrite: email to the owner, a write-once bucket, a printed page. Anything that has an
anchor can later prove the chain up to that point existed. The kernel does not choose where; a bridge can
publish anchors automatically.

## Verifying

```
aether audit verify --org acme [--from 2026-01-01] [--anchor anchor.json]
```

It recomputes each `row_hash` from `prev_hash`, checks `seq` has no gaps, checks every checkpoint signature
against the stored public keys, and, if an anchor file is given, that the anchored head is part of the chain.
Answers: `ok`, or the first `seq` where it breaks and which rule failed (hash mismatch, gap, bad signature,
chain shorter than the anchor). A page in the desk shows the same result and the time of the last good check;
the scheduler runs it nightly and raises a notification on failure.

## Retention without breaking links

The existing purge (`[audit] retention_days`, `access/audit.rs`) deletes old rows. With a chain, purge works up to
a **checkpoint boundary**: rows before it are removed, the checkpoint (with its `head_hash`) stays, and the first
kept row's `prev_hash` equals that head. Verification starts from the stored checkpoint. Purging inside a
checkpoint span is refused.

## History and erasure

Version rows ([History](history.md)) carry the `row_hash` of their audit row. Redacting a field keeps the stored
*hash of the content* in the version, so verification still passes while the content is gone.

## Settings

`audit.chain` (on/off per organization, default on once built), `audit.checkpoint_rows` (10 000),
`audit.checkpoint_secs` (3600), `audit.verify_schedule` (cron, default nightly).

## Tables

New columns on the three audit tables, `audit_chain_head` (one row per org and chain), `audit_checkpoints`,
`audit_keys`.

## Cost

One SHA-256 and one counter update per write. The counter makes writes within one organization commit one at a
time on the audit step. To keep write throughput, a batch of writes in one request takes a block of `seq`
values together. This is measured before it is turned on by default (see *Decide*).

## Not in scope

Public blockchains, trusted timestamping services (an anchor bridge could add one), protecting data rows
themselves (only the audit trail is chained; data history is in [History](history.md)).

## Build order

1. Canonical form and the hash columns, with a unit test vector.
2. Counter and write path; backfill existing rows as "pre-chain".
3. Checkpoints and key storage.
4. `audit verify` (CLI and desk) and the nightly job.
5. Anchor export, then purge at boundaries.

## Decide

| # | Question | Proposal |
|---|---|---|
| 1 | One chain per organization or one global? | **Per organization and per table**: isolation matches the tenant model, and one org's verify never reads another's rows. |
| 2 | Hash algorithm and signature | **SHA-256** and **Ed25519**. |
| 3 | Chain on by default? | **Yes** after the throughput measurement; otherwise opt-in. |
| 4 | Where does the signing key live? | **Local file first**, KMS bridge when available. |
| 5 | Existing rows | **Mark as pre-chain**: verification starts at the first chained row. |
