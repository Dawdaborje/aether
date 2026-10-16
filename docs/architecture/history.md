---
title: "History: versions, point-in-time reads and undo"
description: "Today the kernel keeps an audit row per write (data_access: who, what model, which record ids, no values) and, for models that opt in, a chatter line with the tracked fields that changed."
section: architecture
status: proposal
order: 200
---
# History: versions, point-in-time reads and undo (proposal)

**Status: proposal, nothing built.** Choices are marked *Decide* in the last section.

Today the kernel keeps an audit row per write (`data_access`: who, what model, which record ids, no values)
and, for models that opt in, a [chatter](chatter.md) line with the tracked fields that changed. Neither can
answer "what did this record look like on 3 March" or "put it back". This page adds an opt-in **version store**
that can.

## Turning it on

In the model's JSON, next to `chatter`:

```json
{ "name": "invoice", "history": { "enabled": true, "keep_days": 2555, "fields": "all" } }
```

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `false` | Without it: no table rows, no cost |
| `keep_days` | organization setting | How long versions are kept; `0` keeps forever |
| `fields` | `all` | `all`, or a list of field names to snapshot (others are not stored) |

Fields flagged `encrypted` ([Field encryption](field_encryption.md)) are stored as ciphertext in versions too.

## What is stored

One row per write, in `record_versions`, inside the transaction that `run_audited` already opens
(`facets/core/src/kernel/host/db.rs`), so a version exists if and only if the write committed:

```
record_versions: model_id, record_key, seq (per record, 1..n), at, actor, request_id,
                 op (create|update|delete|restore), snapshot (full) | delta (reverse),
                 audit_ref (the data_access row), prev_hash
```

A **full snapshot** is stored for the first version and every Nth (default 20); between them only the
**reverse delta** (the fields that change going back). Reading version *k* replays at most N-1 deltas. Field
names are stored by stable id (`fld_...`), so a rename never breaks old versions, and a removed field's values
remain readable.

## Reading the past

| Command | Capability | What it does |
|---|---|---|
| `db::get { model, id, as_of }` | `db::query` | the record as it was at `as_of` (timestamp or `seq`); `null` if it did not exist |
| `db::find { ..., as_of }` | `db::query` | a list as of a moment; the filter runs on reconstructed values |
| `history::list { model, id }` | `history::read` | the versions with actor, time, changed fields |
| `history::diff { model, id, from, to }` | `history::read` | field-by-field changes |
| `history::restore { model, id, seq }` | `history::restore` | write the old values back as a new version (`op: restore`) |

Rules apply on the way out: `as_of` reads go through the same record and field rules as a current read
([Rules](rules.md)), evaluated against the **current** rules and the caller's current roles. A field the caller
cannot see now is not shown for the past either. `restore` is an ordinary write: workflow transitions, `checks`
and field write roles all apply, and a restore that would break them is refused with the same error.

`as_of` on `db::find` of a large model is costly. It is limited to indexed filters plus a mandatory `limit`, and
for aggregates over time a plugin should use a report snapshot instead (see [Views and reports](views_reports.md)).

## Undo

The desk and record pages get an **Undo** for the actor's own last change, valid for a short, settable window and
only if nobody wrote the record since. It is `history::restore` to `seq - 1` with that guard; no new mechanism.

## Erasure

A legal erasure request must not be defeated by history. Two tools, both audited:
1. **Redact**: `history::redact { model, id, fields }` replaces those fields in every version with a tombstone.
2. **Crypto-shred**: for `encrypted` fields, destroying the key ([Field encryption](field_encryption.md)) makes every
   version unreadable at once.

The hash links ([Audit chain](audit_chain.md)) cover the *hash of the tombstoned content*, so a redaction does not
break verification: the chain proves "something was here and was redacted", never what.

## Appendix: the road to offline sync

A version row is already an operation log entry (who, when, which fields, on top of which `seq`). Offline sync
would add a **base seq** and per-field merge rules (last writer wins by default, a counter or set type merges).
Nothing in this design blocks it, and nothing in it depends on it. It is listed so the row layout is not painted into
a corner.

## Settings

`history.keep_days` (default for models without their own), `history.snapshot_every` (20), `history.undo_window_secs`
(120), `history.max_as_of_rows` (1000).

## Tables

`record_versions` (above) and `history_redactions` (who redacted what and why). New migration under
`migrations/org/`, after `031_ai.surql`.

## Not in scope

Branching and merging of whole organizations' data, cross-record transactions as one undo, versioning of files in
object storage (files already live in revisions per plugin; media keys are immutable).

## Build order

1. Migration and the write path in `run_audited` (versions only).
2. `db::get as_of`, `history::list` and `diff`.
3. `restore` and Undo.
4. Redact and the retention task (reuse `access/audit.rs::spawn_retention_task`).
5. `db::find as_of`.

## Decide

| # | Question | Proposal |
|---|---|---|
| 1 | Store full rows or deltas only? | **Full every Nth plus reverse deltas**, to bound both space and read cost. |
| 2 | Is `as_of` evaluated under current rules or the rules of that day? | **Current rules.** Safer: history can never reveal more than today's access allows. |
| 3 | Limit `as_of` lists to indexed filters? | **Yes**, plus a `limit`; heavy time analysis belongs to reports. |
| 4 | Redact in place or only crypto-shred? | **Both**: redact for plain fields, shred for encrypted ones. |
| 5 | Undo window length | **120 s**, a setting. |
