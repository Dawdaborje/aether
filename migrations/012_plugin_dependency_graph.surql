-- ================================================
-- Plugin dependency graph
-- Edges: dependent plugin → plugin_depends_on → dependency.
-- Keep plugins.dependencies as a denormalized cache from
-- plugin.toml; the graph is the source for search / install order.
--
-- Examples:
--   RELATE plugins:theme_enterprise -> plugin_depends_on -> plugins:base_components
--     SET requirement = 'required';
--
--   -- What does theme_enterprise need?
--   SELECT ->plugin_depends_on->plugins AS deps FROM plugins:theme_enterprise;
--
--   -- What depends on base_components?
--   SELECT <-plugin_depends_on<-plugins AS dependents FROM plugins:base_components;
--
--   -- Full dependency tree (one hop; recurse in app for install order)
--   SELECT name, ->plugin_depends_on->plugins.name AS depends_on FROM plugins;
-- ================================================

DEFINE TABLE IF NOT EXISTS plugin_depends_on TYPE RELATION IN plugins OUT plugins;

DEFINE FIELD IF NOT EXISTS requirement ON TABLE plugin_depends_on TYPE string DEFAULT "required";
-- "required" | "optional"
DEFINE FIELD IF NOT EXISTS date_created ON TABLE plugin_depends_on TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE plugin_depends_on TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS plugin_depends_on_unique
    ON TABLE plugin_depends_on COLUMNS in, out UNIQUE;
