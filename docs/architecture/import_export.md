---
title: "Import and export"
description: "There is no CSV code in the kernel; facets/report/src/export.rs writes XLSX and a one-page PDF table from rows a plugin already has. Getting data in is the first thing every new organization needs."
section: architecture
status: proposal
order: 210
---
# Import and export (proposal)

**Status: proposal, nothing built.** Choices are marked *Decide* in the last section.

There is no CSV code in the kernel; `facets/report/src/export.rs` writes XLSX and a one-page PDF table from rows a
plugin already has. Getting data in is the first thing every new organization needs. This page designs an importer
whose main promise is: **you see exactly what will happen before it happens, and you can take it back.**

## The flow

1. **Upload** a CSV or XLSX (stored as a media file, size capped).
2. **Map**: the kernel proposes a mapping from column headers to fields (exact name, label, then fuzzy), shows the
   guess, and the user corrects it. Link fields can be matched by title, by an upsert key, or by id.
3. **Dry run**: every row is validated and applied inside one transaction that is then **rolled back**. The
   report says, per row: `create`, `update`, `unchanged`, or `error` with the reason, and totals.
4. **Run**: the same rows are applied for real as a job, in batches, with a callback when done.
5. **Undo**: because each batch's versions are recorded ([History](history.md)), `import::undo` restores the state
   before the run (for models with history on) or deletes what the run created.

## What "validated" means

Row by row, with the same code as any write: type coercion with the organization's locale for dates and numbers,
`required`, `max_length`, `checks`, uniqueness and upsert keys, calculated fields, naming series, **record and
field rules for the importing user**, and workflow `initial` states. An import cannot do what the same person
could not do by hand, and cannot skip a check. Rows that fail are collected, not fatal, unless the run is set to
`stop_on_error`.

## Re-running safely

A mapping can name an **upsert key** (one or more fields, as in [Models](models.md)). Running the same file
twice updates, never duplicates. The job records a `file_hash` and refuses an identical re-run unless told
`force`. Partial failures can be fixed in a downloaded error file (the original row plus a `_error` column) and
re-imported alone.

## Large files

The importer runs in the kernel, not in a plugin, for the reason given in [Deduplication](deduplication.md#why-a-facet-not-a-plugin):
a plugin call is bounded by fuel and memory. Rows are streamed, applied in batches (default 500), and progress
is stored so a restarted server resumes. Concurrency is bounded per organization so an import cannot starve the
server (scheduler queue `import`).

## Export

`export::run { model, filter, fields, format }` produces CSV, XLSX or JSON through the same field and record rules
as a read, so an export contains exactly what the person can see, labelled and formatted per locale (or raw with
`raw: true`). Output goes to storage as a media file with an expiry and an audit row. Reports keep using `report::export`.

## Commands

| Command | Capability | Does |
|---|---|---|
| `import::start { model, media_key, mapping?, mode: dry_run \| run }` | `import::run` | returns a job |
| `import::status` / `import::result` | `import::run` | progress, report, error file key |
| `import::undo { job }` | `import::run` | take back a finished run |
| `import::templates { model }` | `import::run` | a CSV header file generated from the model |
| `export::run` | `export::run` | start an export job |

Both use the job-plus-callback pattern of `ocr_jobs.rs`: a new job kind, an arm in `scheduler/worker.rs::execute`,
and an `import.done` callback event to the plugin that started it.

## Plugin hooks

A model may ship `import.json` to improve the defaults: header aliases (including other languages), value maps
(`"yes" -> true`), default values, columns to ignore, and a sample file. Nothing is required.

## Settings

`import.max_bytes` (50 MiB), `import.max_rows` (1 000 000), `import.batch_size` (500), `import.keep_days`
(files and reports), `import.dry_run_required` (organization can force a clean dry run before a real one).

## Tables

`import_jobs` (who, model, mapping, file hash, status, counters), `import_rows` (per-row outcome, kept for the
retention period), plus version rows if history is on.

## Where it lives

A new `facets/import` crate (parsing, mapping guess, batching), kernel command files
`kernel/host/{import,export}.rs`, capability file `capabilities/import.json`, SDK `../sdks/rust/src/import.rs`,
and a desk/Studio page for the mapping step. CLI: `aether import --org acme --model lead --file leads.csv --dry-run`.

## Not in scope

Importing from another system's database directly (use its export plus a mapping file), scheduled
recurring imports (a [file watch](watch.md) plus `import::start` already does this), bank statement formats
(a bridge per format, later).

## Build order

1. CSV parse, mapping guess, dry run report.
2. Real run as a job, upsert, error file.
3. Undo (needs [History](history.md)).
4. XLSX and exports.

## Decide

| # | Question | Proposal |
|---|---|---|
| 1 | Is a dry run mandatory? | **Optional by default**, a setting makes it mandatory. |
| 2 | Run in the kernel or in a plugin? | **Kernel** (streaming, batching). |
| 3 | Error rows | **Collect and continue**, with an error file; `stop_on_error` is available. |
| 4 | Undo for models without history | **Delete created rows only**; updates cannot be reversed, and the dry run says so. |
| 5 | Same file twice | **Refuse** without `force`. |
