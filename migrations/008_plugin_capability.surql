-- ================================================
-- Plugin capabilities
-- Declared capabilities linked to a plugin catalog
-- row (e.g. "db::query", "email::send").
-- Kernel enforces these at runtime.
-- ================================================

DEFINE TABLE IF NOT EXISTS plugin_capabilities;

DEFINE FIELD IF NOT EXISTS plugin ON TABLE plugin_capabilities TYPE record<plugins>;
-- Capability key, e.g. "db::query", "storage::write"
DEFINE FIELD IF NOT EXISTS s_key ON TABLE plugin_capabilities TYPE string;
DEFINE FIELD IF NOT EXISTS label ON TABLE plugin_capabilities TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS description ON TABLE plugin_capabilities TYPE option<string | NONE>;
-- "required" | "optional"
DEFINE FIELD IF NOT EXISTS requirement ON TABLE plugin_capabilities TYPE string DEFAULT "required";
DEFINE FIELD IF NOT EXISTS date_created ON TABLE plugin_capabilities TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE plugin_capabilities TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS plugin_capabilities_plugin_key
    ON TABLE plugin_capabilities COLUMNS plugin, s_key UNIQUE;
