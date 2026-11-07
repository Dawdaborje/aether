---
title: "Import and export"
description: "Import a CSV or XLSX file into a model with a dry run first, upsert keys, a file of failed rows and undo; export records to CSV or JSON. Every row goes through the same rules as a write by the person importing."
section: architecture
status: stable
order: 210
---
# Import and export

**Status: built.** CSV and XLSX in, CSV and JSON out. Not built: XLSX export, locale-formatted export, expiry of exported files, a Studio page for the mapping step, `aether import` on the command line, `import.json` hooks (header aliases, value maps). They are listed under *Not built* below.

Getting data in is the first thing every new organization needs. The importer's promise: **you see exactly what will happen before it happens, and you can take it back.**

## The flow

1. **Upload** the file as a media file (`POST /api/media`) or put it in the plugin's storage. Size is capped (`import.max_mb`).
2. **Map**: `import::guess` proposes a mapping from column headers to fields (exact name, then label, then a looser match) and reports `unmatched_headers` and `missing_required`. A mapping names `columns` (header to field), an upsert `key`, `defaults`, `values` (value maps), `link_by` (match a link field by a field of the target instead of by id) and `notation` (`decimal_comma`, `date_order` = `dmy`, `mdy` or `ymd`). A key that is not part of the mapping, or a misspelt mapping key, is refused when the import starts.
3. **Dry run** (`mode: "dry_run"`): every row goes through the real write path inside a transaction that ends in `CANCEL TRANSACTION`, so nothing stays. The report says per row `create`, `update`, `unchanged` or `error` with the column, the value and what was expected.
4. **Run** (`mode: "run"`): the same rows are written for real as a job, slice by slice (a slice ends after 90 seconds, a checkpoint is stored, the scheduler continues), and a callback is sent when done.
5. **Undo** (`import::undo`): the job stored the before and after values of every row it changed. Created records are deleted, updated ones get their old values back. A record that was changed after the import is left alone and counted as `skipped`. This does not need the [History](history.md) option.

## What "validated" means

Row by row, with the code of any write: type coercion, `required`, choices, checks, uniqueness, calculated fields, naming series, record and field rules, workflow initial states. The import runs **as the person who started it**, through the plugin's own access: a member whose rules say `owner == $user` can import only rows they own, and the others come back as errors in the rules' words. Failed rows are collected, not fatal, unless `stop_on_error` is set (the run then ends at the first failed row).

## Re-running safely

With a `key`, the same file twice updates nothing and duplicates nothing: unchanged rows are reported as `unchanged`. The job also stores a `file_hash`; an identical file is refused with "already imported" unless `force` is true. Rows that failed are written to `imports/<job>.errors.csv` in the plugin's storage: the original cells plus an `_error` column, ready to be fixed and imported alone.

## Who can see a job

Only the person who started it can read its status and report or undo it (`import_jobs.user`). Other members get the same answer as for a job that does not exist.

## Export

`export::run { model, filter?, fields?, format?, include_id? }` writes CSV (default, UTF-8 with a BOM, so spreadsheets open it correctly) or JSON to the plugin's storage and returns `storage_key`, `rows` and `format`. It reads through the caller's rules, so an export contains exactly what the person can see. The result imports back with the same column names and changes nothing. `export.max_rows` caps it. `report::export` is still the way to get an Excel file or a PDF table of rows a plugin already has.

## Commands

| Command | Capability | Does |
|---|---|---|
| `import::start { model, media_key \| storage_key, mode?, mapping?, sheet?, stop_on_error?, force?, on_done?, ref? }` | `import::run` | returns `{ job }`; `mode` is `dry_run` (default) or `run` |
| `import::status { job }` | `import::run` | state, counters, `mapping_used`, `errors_key`, `error` |
| `import::result { job, offset?, limit?, only_errors? }` | `import::run` | per-row outcomes with `before`/`after`, paged by `next_offset` |
| `import::undo { job }` | `import::run` | take back a finished real run |
| `import::templates { model }` | `import::run` | field list and a CSV header line generated from the model |
| `import::guess { model, headers }` | `import::run` | mapping proposal |
| `export::run` | `export::run` | write an export file |

Lua and Rhai call them as `imports.start`, `imports.status`, ... and `exports.run`; the Rust SDK has `import` and `export` modules. The job is a scheduler job of kind `import` on the queue `import`; when it ends, the function named by `on_done` is called with `{ job, status, ref }`.

## Settings

`import.max_mb` (50), `import.max_rows` (1 000 000), `import.batch_size` (500), `import.dry_run_required` (a real run is refused until a dry run of the same file succeeded), `export.max_rows`. Organizations can override them.

## Tables

`import_jobs` (who, plugin, model, mapping, file hash, status, counters, checkpoint) and `import_rows` (per-row outcome with before and after values), in the organization database (migration `039_imports`). Both are reserved: plugins cannot read or write them.

## Where it lives

`facets/core/src/import/` (`table.rs` the CSV and XLSX readers, `mapping.rs` guess and coercion, `rows.rs` one row through the write path, `job.rs` the sliced job), `kernel/host/import.rs` (the commands), `capabilities/{import,export}.json`, the scheduler arm in `scheduler/worker.rs`, the dry-run flag in `kernel/host/context.rs` and `db.rs`, SDK `../sdks/rust/src/{import,export}.rs`. HTTP end-to-end checks: `tests/e2e/api_tokens/import_e2e.py` (templates, dry run, run, upsert, undo, rules of an ordinary member, XLSX, 3 000 rows, export round trip).

## Not built

XLSX export, locale formatting on export and expiry of exported files; the `import.json` hook file with header aliases and value maps (mappings can carry `values` instead); a Studio or desk page for the mapping step; `aether import --org ... --file ...`; importing from another system's database directly; recurring imports (a [file watch](watch.md) plus `import::start` does this).

## Decisions

| # | Question | Built as |
|---|---|---|
| 1 | Is a dry run mandatory? | Optional; `import.dry_run_required` makes it mandatory. |
| 2 | Run in the kernel or in a plugin? | Kernel, as the starting person, slice by slice. |
| 3 | Error rows | Collected, with an error file; `stop_on_error` is available. |
| 4 | Undo | Always available (stored before and after values), not tied to History; changed-since records are skipped. |
| 5 | Same file twice | Refused without `force`. |
