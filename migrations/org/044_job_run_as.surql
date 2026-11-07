-- ================================================
-- Jobs that run as a person (see docs/architecture/scheduler.md)
--
-- `run_as`: the account (users:…) a plugin job runs as, under that person's roles and record rules, instead of
-- as the kernel. Only a plugin holding `scheduler::run_as` can set it.
-- ================================================
DEFINE FIELD IF NOT EXISTS run_as ON TABLE jobs TYPE option<string | NONE>;
