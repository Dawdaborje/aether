-- ================================================
-- Plugin models
-- Metadata for Surreal tables a plugin owns / may
-- touch. Linked to the plugin catalog row.
-- Actual business tables are created in the org DB
-- when the plugin is installed.
-- ================================================

DEFINE TABLE IF NOT EXISTS plugin_models;

DEFINE FIELD IF NOT EXISTS plugin ON TABLE plugin_models TYPE record<plugins>;
-- Logical model name, e.g. "invoice"
DEFINE FIELD IF NOT EXISTS name ON TABLE plugin_models TYPE string;
-- Physical Surreal table name, e.g. "accounting_invoice"
DEFINE FIELD IF NOT EXISTS table_name ON TABLE plugin_models TYPE string;
DEFINE FIELD IF NOT EXISTS label ON TABLE plugin_models TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS description ON TABLE plugin_models TYPE option<string | NONE>;
-- Optional JSON schema / field map for the DSL & ORM
DEFINE FIELD IF NOT EXISTS schema ON TABLE plugin_models TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE plugin_models TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE plugin_models TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS plugin_models_plugin_name
    ON TABLE plugin_models COLUMNS plugin, name UNIQUE;
DEFINE INDEX IF NOT EXISTS plugin_models_table_name
    ON TABLE plugin_models COLUMNS table_name UNIQUE;
