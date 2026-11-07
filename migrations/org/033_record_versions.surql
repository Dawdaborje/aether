-- ================================================
-- Record versions: what a record looked like after each write (per organization)
--
-- record_versions  one row per write to a record of a model with `history` on, keyed by
--                  (model_id, record_key, seq). `seq` counts from 1 per record.
--                  op: 'baseline' (the record as it was when history first saw it),
--                      'create', 'update', 'delete'.
--                  A row holds either a full `snapshot` (the first version and every Nth after
--                  it) or a `delta` (the fields that changed to a value) with `cleared` (the
--                  fields that became empty). Fields are keyed by stored field id, so a
--                  rename never breaks an old version. A delete row holds neither.
--
-- Written only by the kernel, in the same transaction as the write and its audit row, so a
-- version exists if and only if the write committed. Plugins cannot name this table as a model.
-- See docs/architecture/history.md.
-- ================================================

DEFINE TABLE IF NOT EXISTS record_versions;
DEFINE FIELD IF NOT EXISTS model_id ON TABLE record_versions TYPE string;
DEFINE FIELD IF NOT EXISTS record_key ON TABLE record_versions TYPE string;
DEFINE FIELD IF NOT EXISTS seq ON TABLE record_versions TYPE int;
DEFINE FIELD IF NOT EXISTS op ON TABLE record_versions TYPE string;
DEFINE FIELD IF NOT EXISTS at ON TABLE record_versions TYPE datetime DEFAULT time::now();
-- users:… or visitors:…, or "system" for a job or the kernel
DEFINE FIELD IF NOT EXISTS actor ON TABLE record_versions TYPE string;
DEFINE FIELD IF NOT EXISTS plugin ON TABLE record_versions TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS request_id ON TABLE record_versions TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS snapshot ON TABLE record_versions TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS delta ON TABLE record_versions TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS cleared ON TABLE record_versions TYPE array<string> DEFAULT [];
DEFINE INDEX IF NOT EXISTS record_versions_record ON TABLE record_versions COLUMNS model_id, record_key, seq UNIQUE;
DEFINE INDEX IF NOT EXISTS record_versions_at ON TABLE record_versions COLUMNS model_id, at;
