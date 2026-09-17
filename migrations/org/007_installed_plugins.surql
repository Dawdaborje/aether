-- ================================================
-- Installed plugins
-- Per-org enablement of a platform catalog plugin.
-- Catalog rows live in core `plugins`; this table is
-- "is it installed / configured HERE?".
-- ================================================

DEFINE TABLE IF NOT EXISTS installed_plugins;

-- Matches core plugins.name
DEFINE FIELD IF NOT EXISTS plugin_name ON TABLE installed_plugins TYPE string;
DEFINE FIELD IF NOT EXISTS version ON TABLE installed_plugins TYPE string;
DEFINE FIELD IF NOT EXISTS is_enabled ON TABLE installed_plugins TYPE bool DEFAULT true;
-- Org-specific plugin settings
DEFINE FIELD IF NOT EXISTS config ON TABLE installed_plugins TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS installed_by ON TABLE installed_plugins TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS installed_at ON TABLE installed_plugins TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_created ON TABLE installed_plugins TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE installed_plugins TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS installed_plugins_name
    ON TABLE installed_plugins COLUMNS plugin_name UNIQUE;
