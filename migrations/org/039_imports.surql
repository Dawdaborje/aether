-- ================================================
-- Imports: records read from a spreadsheet file (see docs/architecture/import_export.md)
--
-- import_jobs  one row per import, rehearsal ("dry_run") or real ("run"), or the undo of one: who, which
--              model and file, how the columns were mapped, how far it has got and what it came to.
-- import_rows  what became of each row of the file: created, updated, unchanged or an error with its
--              reason, and for a change the values it replaced (so it can be taken back).
-- Written only by the kernel.
-- ================================================

DEFINE TABLE IF NOT EXISTS import_jobs;
DEFINE FIELD IF NOT EXISTS ulid ON TABLE import_jobs TYPE string;
DEFINE FIELD IF NOT EXISTS plugin ON TABLE import_jobs TYPE string;
DEFINE FIELD IF NOT EXISTS model ON TABLE import_jobs TYPE string;
-- users:… of the person importing; the import is done as them.
DEFINE FIELD IF NOT EXISTS user ON TABLE import_jobs TYPE string;
-- "dry_run" | "run"
DEFINE FIELD IF NOT EXISTS mode ON TABLE import_jobs TYPE string;
-- "queued" | "running" | "done" | "failed" | "undoing" | "undone"
DEFINE FIELD IF NOT EXISTS status ON TABLE import_jobs TYPE string;
-- "media" (an uploaded file) or "storage" (a file in the plugin's own storage)
DEFINE FIELD IF NOT EXISTS source_kind ON TABLE import_jobs TYPE string;
DEFINE FIELD IF NOT EXISTS source_key ON TABLE import_jobs TYPE string;
DEFINE FIELD IF NOT EXISTS sheet ON TABLE import_jobs TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS file_hash ON TABLE import_jobs TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS format ON TABLE import_jobs TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS headers ON TABLE import_jobs TYPE option<array<string> | NONE>;
-- The mapping the person gave, if any; `mapping_used` is the one that was applied (given or guessed).
DEFINE FIELD IF NOT EXISTS mapping ON TABLE import_jobs TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS mapping_used ON TABLE import_jobs TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS guessed ON TABLE import_jobs TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS stop_on_error ON TABLE import_jobs TYPE bool DEFAULT false;
DEFINE FIELD IF NOT EXISTS force ON TABLE import_jobs TYPE bool DEFAULT false;
DEFINE FIELD IF NOT EXISTS callback ON TABLE import_jobs TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS total_rows ON TABLE import_jobs TYPE option<int | NONE>;
-- Rows done so far: a restarted import carries on from here.
DEFINE FIELD IF NOT EXISTS processed ON TABLE import_jobs TYPE int DEFAULT 0;
DEFINE FIELD IF NOT EXISTS created ON TABLE import_jobs TYPE int DEFAULT 0;
DEFINE FIELD IF NOT EXISTS updated ON TABLE import_jobs TYPE int DEFAULT 0;
DEFINE FIELD IF NOT EXISTS unchanged ON TABLE import_jobs TYPE int DEFAULT 0;
DEFINE FIELD IF NOT EXISTS errors ON TABLE import_jobs TYPE int DEFAULT 0;
DEFINE FIELD IF NOT EXISTS error ON TABLE import_jobs TYPE option<string | NONE>;
-- The file of rows that failed, with the reason in a column, to fix and import again.
DEFINE FIELD IF NOT EXISTS errors_key ON TABLE import_jobs TYPE option<string | NONE>;
-- { status, undone, skipped, failed } once an undo has run.
DEFINE FIELD IF NOT EXISTS undo ON TABLE import_jobs TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE import_jobs TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS started_at ON TABLE import_jobs TYPE option<datetime | NONE>;
DEFINE FIELD IF NOT EXISTS finished_at ON TABLE import_jobs TYPE option<datetime | NONE>;
DEFINE INDEX IF NOT EXISTS import_jobs_ulid ON TABLE import_jobs COLUMNS ulid UNIQUE;
DEFINE INDEX IF NOT EXISTS import_jobs_file ON TABLE import_jobs COLUMNS model, file_hash;
DEFINE INDEX IF NOT EXISTS import_jobs_created ON TABLE import_jobs COLUMNS created_at;

DEFINE TABLE IF NOT EXISTS import_rows;
DEFINE FIELD IF NOT EXISTS job ON TABLE import_rows TYPE string;
DEFINE FIELD IF NOT EXISTS row ON TABLE import_rows TYPE int;
-- "create" | "update" | "unchanged" | "error"
DEFINE FIELD IF NOT EXISTS action ON TABLE import_rows TYPE string;
DEFINE FIELD IF NOT EXISTS record_id ON TABLE import_rows TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS message ON TABLE import_rows TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS problems ON TABLE import_rows TYPE option<array<object> | NONE>;
DEFINE FIELD IF NOT EXISTS before ON TABLE import_rows TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS after ON TABLE import_rows TYPE option<object | NONE>;
-- What an undo did with the row: "undone", or "skipped: …" / "failed: …".
DEFINE FIELD IF NOT EXISTS undo ON TABLE import_rows TYPE option<string | NONE>;
DEFINE INDEX IF NOT EXISTS import_rows_row ON TABLE import_rows COLUMNS job, row UNIQUE;
