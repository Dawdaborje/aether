-- ================================================
-- Theme plugins and page layouts (catalog side)
--
-- plugin_themes     the theme a theme plugin ships, validated at load time:
--                   design tokens, which layout renders the app, and its
--                   navigation. Copied into an organization's ui_themes when
--                   the plugin is installed there.
-- plugin_ui_pages.layout   a page may name the layout it wants (for example a
--                   chrome-free "bare" landing page); it overrides the theme's.
-- ================================================

DEFINE TABLE IF NOT EXISTS plugin_themes;
DEFINE FIELD IF NOT EXISTS plugin ON TABLE plugin_themes TYPE record<plugins>;
DEFINE FIELD IF NOT EXISTS name ON TABLE plugin_themes TYPE string;
DEFINE FIELD IF NOT EXISTS label ON TABLE plugin_themes TYPE string;
DEFINE FIELD IF NOT EXISTS is_system ON TABLE plugin_themes TYPE bool DEFAULT false;
-- "light" | "dark" | "system"
DEFINE FIELD IF NOT EXISTS color_mode ON TABLE plugin_themes TYPE string DEFAULT "system";
DEFINE FIELD IF NOT EXISTS tokens ON TABLE plugin_themes TYPE object;
DEFINE FIELD IF NOT EXISTS layout ON TABLE plugin_themes TYPE string DEFAULT "default";
DEFINE FIELD IF NOT EXISTS nav ON TABLE plugin_themes TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE plugin_themes TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS plugin_themes_plugin ON TABLE plugin_themes COLUMNS plugin UNIQUE;

DEFINE FIELD IF NOT EXISTS layout ON TABLE plugin_ui_pages TYPE option<string | NONE>;
