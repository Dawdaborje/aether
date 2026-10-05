-- ================================================
-- Scheduler (per organization)
--
-- scheduled_tasks  recurring work: a plugin's `[[schedule]]` entries (source "manifest") and
--                  tasks it registered while running (source "runtime"). When one is due it
--                  enqueues a job.
-- jobs             the work queue. A job is a plugin function call or a message (email, SMS, …)
--                  to deliver through the configured bridge. Workers claim due jobs, run them and record the outcome; failures
--                  are retried with backoff. Delivery is at-least-once: a job whose worker
--                  died is run again once its lease runs out.
--
-- Written only by the kernel; plugins reach them through `scheduler::*` and cannot name these
-- tables as models.
-- ================================================

DEFINE TABLE IF NOT EXISTS scheduled_tasks;
DEFINE FIELD IF NOT EXISTS plugin ON TABLE scheduled_tasks TYPE string;
DEFINE FIELD IF NOT EXISTS name ON TABLE scheduled_tasks TYPE string;
DEFINE FIELD IF NOT EXISTS function_name ON TABLE scheduled_tasks TYPE string;
-- Exactly one of cron / every_secs is set.
DEFINE FIELD IF NOT EXISTS cron ON TABLE scheduled_tasks TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS every_secs ON TABLE scheduled_tasks TYPE option<int | NONE>;
DEFINE FIELD IF NOT EXISTS timezone ON TABLE scheduled_tasks TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS payload ON TABLE scheduled_tasks TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS queue ON TABLE scheduled_tasks TYPE string DEFAULT 'default';
DEFINE FIELD IF NOT EXISTS max_attempts ON TABLE scheduled_tasks TYPE int DEFAULT 3;
-- "manifest" | "runtime"
DEFINE FIELD IF NOT EXISTS source ON TABLE scheduled_tasks TYPE string DEFAULT 'manifest';
DEFINE FIELD IF NOT EXISTS enabled ON TABLE scheduled_tasks TYPE bool DEFAULT true;
-- After downtime: "once" runs a missed task one time, "skip" waits for the next slot.
DEFINE FIELD IF NOT EXISTS catch_up ON TABLE scheduled_tasks TYPE string DEFAULT 'once';
DEFINE FIELD IF NOT EXISTS next_run ON TABLE scheduled_tasks TYPE datetime;
DEFINE FIELD IF NOT EXISTS last_run ON TABLE scheduled_tasks TYPE option<datetime | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE scheduled_tasks TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE scheduled_tasks TYPE datetime VALUE time::now();
DEFINE INDEX IF NOT EXISTS scheduled_tasks_unique ON TABLE scheduled_tasks COLUMNS plugin, name UNIQUE;
DEFINE INDEX IF NOT EXISTS scheduled_tasks_due ON TABLE scheduled_tasks COLUMNS enabled, next_run;

DEFINE TABLE IF NOT EXISTS jobs;
-- "plugin" (call `function_name` of `plugin`) | "communication" (payload.type: email, sms, …)
DEFINE FIELD IF NOT EXISTS kind ON TABLE jobs TYPE string;
DEFINE FIELD IF NOT EXISTS plugin ON TABLE jobs TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS function_name ON TABLE jobs TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS payload ON TABLE jobs TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS queue ON TABLE jobs TYPE string DEFAULT 'default';
-- "queued" | "running" | "succeeded" | "failed" | "cancelled"
DEFINE FIELD IF NOT EXISTS state ON TABLE jobs TYPE string DEFAULT 'queued';
DEFINE FIELD IF NOT EXISTS run_at ON TABLE jobs TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS attempts ON TABLE jobs TYPE int DEFAULT 0;
DEFINE FIELD IF NOT EXISTS max_attempts ON TABLE jobs TYPE int DEFAULT 3;
DEFINE FIELD IF NOT EXISTS backoff_secs ON TABLE jobs TYPE int DEFAULT 30;
-- The scheduled task that created the job, when one did.
DEFINE FIELD IF NOT EXISTS task ON TABLE jobs TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS unique_key ON TABLE jobs TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS locked_by ON TABLE jobs TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS locked_until ON TABLE jobs TYPE option<datetime | NONE>;
DEFINE FIELD IF NOT EXISTS last_error ON TABLE jobs TYPE option<string | NONE>;
-- Who or what enqueued it (users:…, visitors:…, system:scheduler) and under which request.
DEFINE FIELD IF NOT EXISTS enqueued_by ON TABLE jobs TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS request_id ON TABLE jobs TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE jobs TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_started ON TABLE jobs TYPE option<datetime | NONE>;
DEFINE FIELD IF NOT EXISTS date_finished ON TABLE jobs TYPE option<datetime | NONE>;
DEFINE INDEX IF NOT EXISTS jobs_due ON TABLE jobs COLUMNS state, queue, run_at;
DEFINE INDEX IF NOT EXISTS jobs_plugin ON TABLE jobs COLUMNS plugin, state;
