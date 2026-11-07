-- ================================================
-- Text a plugin asked the kernel to remember for search (`ai::index`), with its embedding
--
-- One row per piece of a source (a policy document, a handbook page). A plugin searches only its own
-- pieces. `model` is the embedding model, because vectors of different models do not compare.
-- Written only by the kernel.
-- ================================================

DEFINE TABLE IF NOT EXISTS ai_chunk SCHEMALESS;
DEFINE FIELD IF NOT EXISTS plugin ON TABLE ai_chunk TYPE string;
DEFINE FIELD IF NOT EXISTS source ON TABLE ai_chunk TYPE string;
DEFINE FIELD IF NOT EXISTS ord ON TABLE ai_chunk TYPE int;
DEFINE FIELD IF NOT EXISTS text ON TABLE ai_chunk TYPE string;
DEFINE FIELD IF NOT EXISTS model ON TABLE ai_chunk TYPE string;
DEFINE FIELD IF NOT EXISTS embedding ON TABLE ai_chunk TYPE array<float>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE ai_chunk TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS ai_chunk_source ON TABLE ai_chunk COLUMNS plugin, source;
