-- A version written by `history::restore` or `history::undo` (op = 'restore') says which version
-- the record was put back to. See docs/architecture/history.md.
DEFINE FIELD IF NOT EXISTS restored_from ON TABLE record_versions TYPE option<int | NONE>;
