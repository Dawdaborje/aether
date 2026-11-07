-- Record history: how long each version is kept, and the log of redactions.
--
-- record_versions.keep_days  the model's `history.keep_days` when the version was written: 0 keeps
--                            it forever, absent means the organization setting `history.keep_days`.
-- history_redactions         who blotted out which fields of a record's history, and why. The
--                            values are gone from the versions (and from the chatter lines that
--                            tracked them); this row says it was done. Written only by the kernel.
-- See docs/architecture/history.md.
DEFINE FIELD IF NOT EXISTS keep_days ON TABLE record_versions TYPE option<int | NONE>;
DEFINE INDEX IF NOT EXISTS record_versions_keep ON TABLE record_versions COLUMNS keep_days, at;

DEFINE TABLE IF NOT EXISTS history_redactions;
DEFINE FIELD IF NOT EXISTS model_id ON TABLE history_redactions TYPE string;
DEFINE FIELD IF NOT EXISTS record_key ON TABLE history_redactions TYPE string;
-- Stored field ids.
DEFINE FIELD IF NOT EXISTS fields ON TABLE history_redactions TYPE array<string>;
DEFINE FIELD IF NOT EXISTS reason ON TABLE history_redactions TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS actor ON TABLE history_redactions TYPE string;
DEFINE FIELD IF NOT EXISTS at ON TABLE history_redactions TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS history_redactions_record ON TABLE history_redactions COLUMNS model_id, record_key;
