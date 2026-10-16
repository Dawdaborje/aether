-- ================================================
-- Document parsing results (per organization)
--
-- What `ocr::parse` and `ocr::submit` produced for a stored file: one row per (file, engine, options),
-- keyed by `cache_key`. A media key is never reused for different bytes, so a row is valid for ever;
-- it is the cache that makes a repeated parse free, the answer of a parse that ran as a job, and the
-- list of plugin functions to call when it finishes. Written only by the kernel.
-- ================================================

-- Schemaless: `options`, `result` and `callbacks` hold nested JSON whose shape belongs to the engine.
DEFINE TABLE IF NOT EXISTS ocr_result SCHEMALESS;
DEFINE FIELD IF NOT EXISTS cache_key ON TABLE ocr_result TYPE string;
DEFINE FIELD IF NOT EXISTS media_key ON TABLE ocr_result TYPE string;
DEFINE FIELD IF NOT EXISTS engine ON TABLE ocr_result TYPE string;
DEFINE FIELD IF NOT EXISTS options ON TABLE ocr_result TYPE object;
-- pending: waiting for a worker; running; done; failed.
DEFINE FIELD IF NOT EXISTS status ON TABLE ocr_result TYPE string DEFAULT 'pending';
DEFINE FIELD IF NOT EXISTS result ON TABLE ocr_result TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS error ON TABLE ocr_result TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS pages ON TABLE ocr_result TYPE option<int | NONE>;
DEFINE FIELD IF NOT EXISTS ocr_used ON TABLE ocr_result TYPE option<bool | NONE>;
-- `{ plugin, function, ref }` to call when the job ends.
DEFINE FIELD IF NOT EXISTS callbacks ON TABLE ocr_result TYPE array DEFAULT [];
DEFINE FIELD IF NOT EXISTS requested_by ON TABLE ocr_result TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE ocr_result TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE ocr_result TYPE datetime VALUE time::now();
DEFINE INDEX IF NOT EXISTS ocr_result_cache_key ON TABLE ocr_result COLUMNS cache_key UNIQUE;
DEFINE INDEX IF NOT EXISTS ocr_result_status ON TABLE ocr_result COLUMNS status;
