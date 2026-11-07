---
title: "Audit chain: tamper-evident audit"
description: "The audit trail is written in the same transaction as the work, so a write without an audit row cannot happen. The chain makes it detectable when someone later changes, removes or reorders audit rows."
section: architecture
status: stable
order: 50
---
# Audit chain: tamper-evident audit

**Status: built** (sealing, checkpoints, verification, anchors, purge at a checkpoint, daily check, CLI). Not built: a
notification when a check fails (it is logged as an error), keeping the audit key in a KMS.
Choices that differ from the first proposal are in the last section.

The audit trail ([Access](access.md#the-audit-trail)) is written in the same transaction as the work, so a write
without an audit row cannot happen. But the rows themselves are ordinary rows: an administrator with database access
could edit or delete them, and nobody could tell. The chain makes any such change **detectable**, by anyone, with
only the database and a public key.

It does not stop a database administrator from changing data. It guarantees they cannot do so silently.

## How it works

The three audit tables (`data_access`, `plugin_calls`, `page_visits`) are three chains. A background **sealer**
gives the rows that are not sealed yet a number (`chain_seq`: 1, 2, 3 ... per table) and links each to the one
before:

```
row_hash = SHA-256( canonical form of the row, which includes the previous row's hash and its own number )
```

Changing, removing or reordering a sealed row breaks every link after it. The exact canonical form (field order,
how a missing value, a list and a separator are written) is specified in the module documentation of
`facets/core/src/access/chain.rs`, with a unit test that fixes it, so a verifier in any language can be written.

### Why the sealer is not in the request path

The first proposal numbered and hashed each row inside the write's own transaction. That would have made every
audited operation in an organization wait for one counter row, and **reads are audited too**: every page load would
queue behind it. So the audit row is written exactly as before, and the sealer, a task running every few seconds,
numbers it afterwards. The cost is a short **window** (seconds) between a row being written and being sealed.
Verification reports how many rows are in it (`unsealed`); a row changed inside the window is simply sealed as
changed, so the chain proves nothing about it. Nothing on the request path changed, and there is no throughput cost.

The sealer takes the rows with no `chain_seq`, ordered by time, a batch at a time (500), and commits the batch and the
new head together in one transaction that fails if another server sealed first. Several servers can run it at once:
whichever commits first wins, and the other does nothing that pass.

## Checkpoints

When `audit.checkpoint_rows` rows (10 000) have been sealed since the last one, or the last one is older than
`audit.checkpoint_secs` (3600), the sealer makes a **checkpoint**: a statement "up to row `to_seq` the chain's hash
was `head_hash`", signed with Ed25519:

```
signature over  aether-audit-checkpoint-v1|<org>|<chain>|<to_seq>|<head_hash>|<at_ns>
```

The signing key is the installation's audit key, `<app_dir>/conf/audit.key` (32 random bytes in hex, mode 0600,
made the first time the server starts; back it up with the secret key). The public key goes into the organization's
`audit_keys` table with the first checkpoint it signs, so checkpoints keep verifying if the key is replaced later.

## Anchors

`aether --audit-export-anchor DIR --org <database>` writes the newest checkpoint of each chain as a small JSON file
(`<chain>.anchor.json`). Keep it somewhere the server's administrator cannot rewrite: email it to the owner, put it
in a write-once bucket, print it. Anything that has an anchor can later prove the chain up to that point existed
unchanged, **even if** the attacker rewrote the chain, its head and its checkpoints to agree with each other.

## Verifying

```
aether --audit-verify --org <database> [--audit-anchor data_access.anchor.json]
```

For each chain it recomputes every row's hash from the one before it and checks:

| Rule | Means |
|---|---|
| `hash` | a row's content no longer matches its stored hash |
| `link` | a row does not link to the one before it (a row was replaced or reordered) |
| `gap` | a number is missing or doubled (a row was removed or added) |
| `head` | the chain ends before its head says (the last rows were removed) |
| `signature`, `unknown_key` | a checkpoint was not signed by a key on record |
| `checkpoint` | a checkpoint names a row that is gone, or has another hash |
| `anchor` | the anchor is forged, or the chain is shorter than it, or its row differs |

It prints one block per chain with the first failure and exits with 1 if anything is wrong. The server also checks
every chain of every organization once a day (the first time ten minutes after start-up), keeps the result in
`audit_verifications` and logs a broken chain as an **error**.

## In the desk

An organization's **Audit log** tab (`/desk/organizations/<db>/audit`) shows the three audit tables and the chain's
state. The banner says whether the last check found the chain intact, and **Check now** runs the same verification as
the daily one. The tables can be filtered by text, actor, plugin, operation (data access), refused or failed requests
(calls and visits) and time range; a row opens to show its record ids, statement, address and chain number, and
**Show this request** lists every row that request wrote, so switching tabs follows one request across the tables.
The filters are kept in the address. The page only reads the tables, and is open to a developer of the organization.

- `GET /api/ui/organizations/{db}/audit?table=&q=&actor=&plugin=&request=&operation=&status=&failed=&from=&until=&offset=&limit=`
- `GET /api/ui/organizations/{db}/audit/chain` the chain heads and the last verification
- `POST /api/ui/organizations/{db}/audit/verify` check every chain now

## Retention without breaking links

`[audit] retention_days` ([Access](access.md#the-audit-trail)) still removes old audit rows, but for a chain that has
started it removes **the longest prefix that ends at a checkpoint whose row is older than that**. The chain then
starts from that checkpoint's hash (`base_seq`, `base_hash` in `audit_chain_head`), so what is left still verifies.
Unsealed rows are never removed. A table with no chain yet is purged by date as before.

## Existing rows

The first time the sealer runs in an organization, the rows already there are sealed in time order. The chain vouches
for them from that moment on, not before: it cannot prove they were not changed earlier.

## Settings

| Setting | Default | Means |
|---|---|---|
| `audit.chain` | on | seal this organization's audit rows (the global value can be overridden per organization) |
| `audit.checkpoint_rows` | 10 000 | make a checkpoint after this many sealed rows |
| `audit.checkpoint_secs` | 3600 | or when the last is this old |

The sealer's pace (5 seconds, a full visit of every chain every 5 minutes) and batch size are constants in
`chain_service.rs`.

## Tables

New columns `chain_seq`, `prev_hash`, `row_hash` on the three audit tables, and `audit_chain_head`,
`audit_checkpoints`, `audit_keys`, `audit_verifications` (migration `036_audit_chain.surql`).

## Where it lives

`facets/core/src/access/chain.rs` (canonical form, keys, sealing, checkpoints, verification, anchors, purge),
`chain_service.rs` (the background tasks, started with `--serve`), `access/audit.rs` (the purge), `aether/src/cli/audit.rs`
(the flags).

## Not in scope

Public blockchains, trusted timestamping (an anchor bridge could add one), protecting data rows themselves (only the
audit trail is chained; the history of data is in [History](history.md)), a notification when a check fails.

## Decide

| # | Question | Decision |
|---|---|---|
| 1 | One chain per organization or one global? | **Per organization and per table**: isolation matches the tenant model, and one organization's verify never reads another's rows. |
| 2 | Hash algorithm and signature | **SHA-256** and **Ed25519**. |
| 3 | Chain on by default? | **Yes.** The throughput concern that made this conditional went away when sealing moved off the request path. |
| 4 | Where does the signing key live? | **A local file first**; a KMS bridge when [field encryption](field_encryption.md) brings one. |
| 5 | Existing rows | **Sealed in time order at the first pass**; the chain vouches for them from then on (this replaces "mark as pre-chain"). |
| 6 | Hash in the write transaction or afterwards? | **Afterwards, by the sealer** (see above). |
| 7 | Subcommands (`aether audit verify`) or flags? | **Flags** (`--audit-verify`, `--audit-export-anchor`, `--audit-anchor`), beside `--purge-audit`: the CLI has no subcommands, and this keeps it that way. |
