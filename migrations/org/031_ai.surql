-- ================================================
-- AI results and usage (per organization)
--
-- `ai_result` holds what `ai::submit` produced: one row per submitted conversation (a model answer
-- is not repeatable, so it is never shared between requests), the answer, and the list of plugin
-- functions to call when it finishes. `ai_usage` is the log behind the token budgets and the usage
-- page: who asked, through which plugin, which model, how many tokens, how long, whether it worked.
-- It never holds a prompt or an answer. Both are written only by the kernel.
-- ================================================

DEFINE TABLE IF NOT EXISTS ai_result SCHEMALESS;
DEFINE FIELD IF NOT EXISTS job_id ON TABLE ai_result TYPE string;
-- pending: waiting for a worker; running; done; failed.
DEFINE FIELD IF NOT EXISTS status ON TABLE ai_result TYPE string DEFAULT 'pending';
DEFINE FIELD IF NOT EXISTS result ON TABLE ai_result TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS error ON TABLE ai_result TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS plugin ON TABLE ai_result TYPE option<string | NONE>;
-- `{ plugin, function, ref }` to call when the job ends.
DEFINE FIELD IF NOT EXISTS callbacks ON TABLE ai_result TYPE array DEFAULT [];
DEFINE FIELD IF NOT EXISTS requested_by ON TABLE ai_result TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE ai_result TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE ai_result TYPE datetime VALUE time::now();
DEFINE INDEX IF NOT EXISTS ai_result_job_id ON TABLE ai_result COLUMNS job_id UNIQUE;
DEFINE INDEX IF NOT EXISTS ai_result_status ON TABLE ai_result COLUMNS status;

DEFINE TABLE IF NOT EXISTS ai_usage SCHEMALESS;
-- The UTC day, `2026-10-16`, so a daily budget is one indexed sum.
DEFINE FIELD IF NOT EXISTS day ON TABLE ai_usage TYPE string;
DEFINE FIELD IF NOT EXISTS who ON TABLE ai_usage TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS plugin ON TABLE ai_usage TYPE option<string | NONE>;
-- complete or embed.
DEFINE FIELD IF NOT EXISTS kind ON TABLE ai_usage TYPE string;
DEFINE FIELD IF NOT EXISTS provider ON TABLE ai_usage TYPE string;
DEFINE FIELD IF NOT EXISTS model ON TABLE ai_usage TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS input_tokens ON TABLE ai_usage TYPE int DEFAULT 0;
DEFINE FIELD IF NOT EXISTS output_tokens ON TABLE ai_usage TYPE int DEFAULT 0;
DEFINE FIELD IF NOT EXISTS tokens ON TABLE ai_usage TYPE int DEFAULT 0;
DEFINE FIELD IF NOT EXISTS duration_ms ON TABLE ai_usage TYPE int DEFAULT 0;
DEFINE FIELD IF NOT EXISTS ok ON TABLE ai_usage TYPE bool DEFAULT true;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE ai_usage TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS ai_usage_day ON TABLE ai_usage COLUMNS day;
