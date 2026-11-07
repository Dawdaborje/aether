-- ================================================
-- Installed plugin dependency edges (per org)
-- Mirrors core plugin_depends_on for the subset of
-- plugins actually installed in this tenant.
-- ================================================

DEFINE TABLE IF NOT EXISTS installed_plugin_depends_on
    TYPE RELATION IN installed_plugins OUT installed_plugins;

DEFINE FIELD IF NOT EXISTS requirement ON TABLE installed_plugin_depends_on TYPE string DEFAULT "required";
DEFINE FIELD IF NOT EXISTS date_created ON TABLE installed_plugin_depends_on TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE installed_plugin_depends_on TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS installed_plugin_depends_on_unique
    ON TABLE installed_plugin_depends_on COLUMNS in, out UNIQUE;
