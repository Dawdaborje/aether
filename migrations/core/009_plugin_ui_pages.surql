-- ================================================
-- Plugin UI pages
-- Pages declared by a plugin (XML DSL),
-- registered in the platform catalog.
-- ================================================

DEFINE TABLE IF NOT EXISTS plugin_ui_pages;

DEFINE FIELD IF NOT EXISTS plugin ON TABLE plugin_ui_pages TYPE record<plugins>;
DEFINE FIELD IF NOT EXISTS route ON TABLE plugin_ui_pages TYPE string;
DEFINE FIELD IF NOT EXISTS title ON TABLE plugin_ui_pages TYPE string;
-- Logical model this page binds to, e.g. "accounting.invoice"
DEFINE FIELD IF NOT EXISTS model ON TABLE plugin_ui_pages TYPE option<string | NONE>;
-- "list" | "form" | "kanban" | "dashboard" | "custom"
DEFINE FIELD IF NOT EXISTS view_type ON TABLE plugin_ui_pages TYPE option<string | NONE>;
-- Compiled component tree JSON from XML page definitions
DEFINE FIELD IF NOT EXISTS component_tree ON TABLE plugin_ui_pages TYPE option<object | NONE>;
-- Source path of the .xml file inside the package
DEFINE FIELD IF NOT EXISTS source_path ON TABLE plugin_ui_pages TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS icon ON TABLE plugin_ui_pages TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS menu_group ON TABLE plugin_ui_pages TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS sort_order ON TABLE plugin_ui_pages TYPE option<int | NONE> DEFAULT 0;
DEFINE FIELD IF NOT EXISTS is_menu_visible ON TABLE plugin_ui_pages TYPE bool DEFAULT true;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE plugin_ui_pages TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE plugin_ui_pages TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS plugin_ui_pages_plugin_route
    ON TABLE plugin_ui_pages COLUMNS plugin, route UNIQUE;
