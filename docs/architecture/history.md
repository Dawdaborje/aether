---
title: "History: versions, point-in-time reads and undo"
description: "Today the kernel keeps an audit row per write (data_access: who, what model, which record ids, no values) and, for models that opt in, a chatter line with the tracked fields that changed."
section: architecture
status: stable
order: 200
---
# History: versions, point-in-time reads and undo

**Status: partly built.** Built: the model switch (`history`), the `record_versions` table and write path
(migrations `033_record_versions.surql` and `034_record_versions_restore.surql`, `history_extra` in
`facets/core/src/kernel/host/db.rs`), reading the past (`db::get` with `as_of`, `history::list`, `history::diff`),
and putting a record back (`history::restore`, `history::undo`), in `facets/core/src/kernel/host/history.rs` with the
pure replay in `facets/core/src/data_model/history.rs`; searching the past (`db::find` with `as_of`);
redaction (`history::redact`); retention (`facets/core/src/history_retention.rs`, an hourly task); and the Undo
action and the model editor in the web app. Offline sync stays an appendix. Choices are marked *Decide* in the
last section.

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
record_versions: model_id, record_key, seq (per record, 1..n), at, actor, plugin, request_id,
                 op (baseline|create|update|delete|restore), restored_from (for a restore),
                 snapshot (full) | delta (what changed to a value) + cleared (what became empty)
```

`prev_hash` and a link to the audit row are added with the [Audit chain](audit_chain.md).

A **full snapshot** is stored for the first version and every Nth (default 20, `history.snapshot_every`); between
them only the **forward delta**: the fields that changed to a value (`delta`) and the fields that became empty
(`cleared`). Reading version *k* starts at the nearest snapshot at or before *k* and applies at most N-1 deltas
forward, so the past never depends on the current row. Field names are stored by stable id (`fld_...`), so a
rename never breaks old versions, and a removed field's values remain readable.

Every `update` version records its delta, also the ones that hold a snapshot, so `history::list` can always say what
changed. A write that changes none of the kept fields leaves no version. A record that existed before history was switched
on gets a `baseline` version (its state before the first write history sees) so its earlier state is not lost.
Deleting writes a `delete` version with no content; the content lives in the versions before it. With
`fields: [..]`, only those fields are kept and a change to any other is not a version.

Concurrent writes to one record collide on the unique `(model_id, record_key, seq)` index; the second write fails
as a database conflict, as two concurrent writes to one record already do.

## Reading the past

| Command | Capability | What it does |
|---|---|---|
| `db::get { model, id, as_of }` | `db::query` | **Built.** The record as it was at `as_of`: a time (RFC 3339) or `{ "seq": n }`. `null` if it did not exist then, if there is no such version, or if the caller may not read it. Cannot be combined with `expand` |
| `history::list { model, id, limit?, before_seq? }` | `history::read` | **Built.** The versions, newest first: `seq`, `op`, `at`, `actor`, `plugin`, `request_id` and `fields` (names of what changed). `next` is the `seq` to pass as `before_seq` for the next page |
| `history::diff { model, id, from, to? }` | `history::read` | **Built.** Field by field: `{ field, before, after }` (`to` defaults to the newest version) |
| `db::find { ..., as_of }` | `db::query` | **Built.** The records that matched `filter` at that time, among those the caller may read today. Takes `filter` and `limit` only (`order`, `offset`, `expand` and a `{ seq }` are refused). Bounded: see below |
| `history::restore { model, id, seq }` | `history::restore` (and `db::mutate`) | **Built.** Put the record back to how it was after version `seq`, as one new version (`op: restore`, `restored_from: seq`). Answers `{ data, restored, from_seq }`; `restored: false` when nothing differs |
| `history::undo { model, id }` | `history::restore` (and `db::mutate`) | **Built.** Take back the caller's own last change (see Undo) |

`as_of` by time picks the newest version at or before that moment. A record that history met already existing (its
first version is a `baseline`) is answered from that baseline for any earlier time, and the answer says
`from_baseline: true`: it is the state before the first change history saw, not a proven state at that time. For a
record created with history on, a time before its creation answers `null`.

Rules apply on the way out (**built**): `as_of` reads go through the same record and field rules as a current read
([Rules](rules.md)), evaluated against the **current** rules and the caller's current roles. The old state is checked
as a row against today's record rule, so a person who may not read the record today gets `null` (`as_of`, `diff`) or
an empty list (`list`) for its whole past; a field the caller cannot see now is not shown for the past either, and
is left out of `fields` and `diff`. The check and the read are one audited read of the record. Link fields are stored
as text (`table:key`), so a rule over them behaves the same on a past state as on a current one. `restore` is an ordinary write: workflow transitions, `checks`
and field write roles all apply, and a restore that would break them is refused with the same error.

### Searching the past

`db::find` with `as_of` rebuilds every record that existed at that time and filters the old states, under the same
audited read as a single `as_of` read. That is costly, so it is bounded rather than indexed: it refuses a model
with more than `history.max_as_of_rows` records alive then (default 1000), or whose versions up to then number more
than 20 000, and says to read single records or use a report snapshot instead (see
[Views and reports](views_reports.md)). It returns rows by record key; a record that history only met later (its
first version is a `baseline` newer than `as_of`) is not found. Counting and grouping the past are not available.

### What a restore writes

An ordinary update, so the model's `checks`, its workflow (putting a `state` back must be an allowed move) and the
caller's record and field rules all apply, and a restore that would break one is refused with that error and changes
nothing. Only the fields that differ are written. Left alone: fields the caller cannot read, fields the kernel
calculates or numbers, and (for a model with `fields: [..]`) the fields the versions do not keep. A field that was
empty then is emptied now. A deleted record is not brought back (`history::restore` answers that the record is gone).

## Undo

`history::undo` takes back the caller's **own** last change: the newest version must be an `update` or `restore` made
by the caller, no older than `history.undo_window_secs` (default 120), with an earlier version to go back to. If
anyone wrote the record since, the newest version is theirs and undo is refused. It is `history::restore` to
`seq - 1`; nothing else is new. On record pages (a form view of a model with `history`) and in the list's Actions menu the web app offers
**Undo**: it calls the kernel record action `kernel.undo_records`, which may use `history::undo` without the plugin
declaring the capability, and still runs through the plugin's model grants, the model's rules and the audit. The page
knows the model keeps a history because the page compiler marks its views (`modelHistory`).

## Erasure

A legal erasure request must not be defeated by history. Two tools, both audited:
1. **Redact** (built): `history::redact { model, id, fields, reason? }` replaces those fields with `"[redacted]"` in
   every version and in the chatter lines that tracked them, and writes a row to `history_redactions` (who, when,
   which fields, why). Only an administrator may (`history::redact` capability and the `org_admin` role). The versions
   stay, saying who changed the record and when. It does not change the record itself (update it first), and it
   cannot be undone.
2. **Crypto-shred**: for `encrypted` fields, destroying the key ([Field encryption](field_encryption.md)) makes every
   version unreadable at once.

The hash links ([Audit chain](audit_chain.md)) cover the *hash of the tombstoned content*, so a redaction does not
break verification: the chain proves "something was here and was redacted", never what.

## Appendix: the road to offline sync

A version row is already an operation log entry (who, when, which fields, on top of which `seq`). Offline sync
would add a **base seq** and per-field merge rules (last writer wins by default, a counter or set type merges).
Nothing in this design blocks it, and nothing in it depends on it. It is listed so the row layout is not painted into
a corner.

## Retention

A version is kept for the model's `history.keep_days`, or for the organization setting `history.keep_days` when the
model does not say; `0` keeps forever, also for a model that says `0` when the organization says more. The keep time
is written into each version when it is made (`record_versions.keep_days`), so changing a model's keep time applies
to new versions. An hourly task (`history_retention::spawn_cleanup`, first pass 90 s after start) trims, a bounded
batch of records at a time:

1. The newest version older than the cut-off becomes a full snapshot (its state is replayed from the versions
   before it) and the versions before it are removed. The record can still be read as it was from the cut-off on.
2. A record deleted longer ago than its keep time loses its whole history.

Redacted values stay redacted; the live record is never touched.

## Settings

`history.keep_days` (default for models without their own, 0), `history.undo_window_secs` (120),
`history.max_as_of_rows` (1000). A model's `snapshot_every` (default 20) and `keep_days` are in its `history` block.

## Tables

`record_versions` (above) and `history_redactions` (who redacted what and why): migrations `033`, `034` and `035`
under `migrations/org/`.

## Not in scope

Branching and merging of whole organizations' data, cross-record transactions as one undo, versioning of files in
object storage (files already live in revisions per plugin; media keys are immutable).

## Build order

1. Migration and the write path in `run_audited` (versions only). **Built.**
2. `db::get as_of`, `history::list` and `diff`. **Built.**
3. `restore` and Undo. **Built** (no web button yet).
4. Redact and the retention task. **Built.**
5. `db::find as_of`. **Built.**
6. Web: `history` in the model editor, and an Undo action on record pages for models that keep a history. **Built.**

## Decide

| # | Question | Proposal |
|---|---|---|
| 1 | Store full rows or deltas only? | **Full every Nth plus forward deltas** (built), to bound both space and read cost without needing the current row to read the past. |
| 2 | Is `as_of` evaluated under current rules or the rules of that day? | **Current rules.** Safer: history can never reveal more than today's access allows. |
| 3 | Limit `as_of` lists to indexed filters? | **Bounded instead** (built): a record cap and a version cap, plus a `limit`; heavy time analysis belongs to reports. |
| 4 | Redact in place or only crypto-shred? | **Both**: redact for plain fields, shred for encrypted ones. |
| 5 | Undo window length | **120 s**, a setting. |
