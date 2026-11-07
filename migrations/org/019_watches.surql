-- ================================================
-- Watches (per organization)
--
-- The folders a plugin asked to be told about, copied from its manifest when it is installed or
-- upgraded. A scheduler node listens to them (inotify on Linux) and turns each change into a job
-- that calls the plugin's function. Written only by the kernel.
-- ================================================

DEFINE TABLE IF NOT EXISTS watches;
DEFINE FIELD IF NOT EXISTS plugin ON TABLE watches TYPE string;
DEFINE FIELD IF NOT EXISTS name ON TABLE watches TYPE string;
-- Relative to the plugin's folder, `orgs/<organization>/plugins/<plugin>`.
DEFINE FIELD IF NOT EXISTS path ON TABLE watches TYPE string DEFAULT '';
DEFINE FIELD IF NOT EXISTS function_name ON TABLE watches TYPE string;
DEFINE FIELD IF NOT EXISTS events ON TABLE watches TYPE array<string>;
DEFINE FIELD IF NOT EXISTS pattern ON TABLE watches TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS debounce_ms ON TABLE watches TYPE int DEFAULT 500;
DEFINE FIELD IF NOT EXISTS recursive ON TABLE watches TYPE bool DEFAULT false;
DEFINE FIELD IF NOT EXISTS scan_on_start ON TABLE watches TYPE bool DEFAULT false;
DEFINE FIELD IF NOT EXISTS poll_secs ON TABLE watches TYPE option<int | NONE>;
DEFINE FIELD IF NOT EXISTS queue ON TABLE watches TYPE string DEFAULT 'default';
DEFINE FIELD IF NOT EXISTS max_attempts ON TABLE watches TYPE int DEFAULT 3;
DEFINE FIELD IF NOT EXISTS enabled ON TABLE watches TYPE bool DEFAULT true;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE watches TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE watches TYPE datetime VALUE time::now();
DEFINE INDEX IF NOT EXISTS watches_unique ON TABLE watches COLUMNS plugin, name UNIQUE;
